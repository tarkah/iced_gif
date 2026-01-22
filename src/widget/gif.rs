//! Display a GIF in your user interface
use std::fmt;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

use iced_runtime::Task;
use iced_widget::core::border;
#[allow(unused)]
use iced_widget::core::image::Image;
use iced_widget::core::image::{self, FilterMethod, Handle};
use iced_widget::core::mouse::Cursor;
use iced_widget::core::widget::{tree, Tree};
use iced_widget::core::{
    layout, renderer, window, Clipboard, ContentFit, Element, Event, Layout, Length, Rectangle,
    Rotation, Shell, Size, Widget,
};
use image_rs::codecs::gif;
use image_rs::{AnimationDecoder, ImageDecoder};

#[cfg(not(feature = "tokio"))]
use iced_futures::futures::{AsyncRead, AsyncReadExt};
#[cfg(feature = "tokio")]
use tokio::io::{AsyncRead, AsyncReadExt};

/// Error loading or decoding a gif
#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    /// Decode error
    #[error(transparent)]
    Image(#[from] std::sync::Arc<image_rs::ImageError>),
    /// Load error
    #[error(transparent)]
    Io(#[from] std::sync::Arc<std::io::Error>),
    #[cfg(feature = "tokio")]
    /// Tokio join error
    #[error(transparent)]
    BlockingTask(#[from] std::sync::Arc<tokio::task::JoinError>),
}

impl std::convert::From<image_rs::ImageError> for Error {
    fn from(value: image_rs::ImageError) -> Self {
        Self::Image(std::sync::Arc::new(value))
    }
}

impl std::convert::From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(std::sync::Arc::new(value))
    }
}

#[cfg(feature = "tokio")]
impl std::convert::From<tokio::task::JoinError> for Error {
    fn from(value: tokio::task::JoinError) -> Self {
        Self::BlockingTask(std::sync::Arc::new(value))
    }
}

/// The frames of a decoded gif
#[derive(Clone)]
pub struct Frames {
    first: Frame,
    frames: Vec<Frame>,
    total_bytes: u64,
}

impl fmt::Debug for Frames {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Frames").finish()
    }
}

impl Frames {
    /// Load [`Frames`] from the supplied path
    pub fn load_from_path(path: impl AsRef<Path>) -> Task<Result<Frames, Error>> {
        #[cfg(feature = "tokio")]
        use tokio::fs::File;
        #[cfg(feature = "tokio")]
        use tokio::io::BufReader;

        #[cfg(not(feature = "tokio"))]
        use async_fs::File;
        #[cfg(not(feature = "tokio"))]
        use iced_futures::futures::io::BufReader;

        let path = path.as_ref().to_path_buf();

        let f = async move {
            let reader = BufReader::new(File::open(path).await?);

            Self::from_reader(reader).await
        };

        Task::perform(f, std::convert::identity)
    }

    /// Decode [`Frames`] from the supplied async reader
    pub async fn from_reader<R: AsyncRead>(reader: R) -> Result<Self, Error> {
        use iced_futures::futures::pin_mut;

        pin_mut!(reader);

        let mut bytes = vec![];

        reader.read_to_end(&mut bytes).await?;

        #[cfg(not(feature = "tokio"))]
        return Self::from_bytes(bytes);

        #[cfg(feature = "tokio")]
        return Self::from_bytes(bytes).await;
    }

    #[cfg(not(feature = "tokio"))]
    /// Decode [`Frames`] from the supplied bytes
    pub fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        let decoder = gif::GifDecoder::new(io::Cursor::new(bytes))?;

        let total_bytes = decoder.total_bytes();

        let frames = decoder
            .into_frames()
            .into_iter()
            .map(|result| result.map(Frame::from))
            .collect::<Result<Vec<_>, _>>()?;

        let first = frames.first().cloned().unwrap();

        Ok(Frames {
            total_bytes,
            first,
            frames,
        })
    }

    #[cfg(feature = "tokio")]
    /// Decode [`Frames`] from the supplied bytes.
    ///
    /// Note: This function wraps the decoding step with [`tokio::task::spawn_blocking`].
    pub async fn from_bytes(bytes: Vec<u8>) -> Result<Self, Error> {
        let decoder =
            tokio::task::spawn_blocking(move || gif::GifDecoder::new(io::Cursor::new(bytes)))
                .await??;

        let total_bytes = decoder.total_bytes();

        let frames = decoder
            .into_frames()
            .into_iter()
            .map(|result| result.map(Frame::from))
            .collect::<Result<Vec<_>, _>>()?;

        let first = frames.first().cloned().unwrap();

        Ok(Frames {
            total_bytes,
            first,
            frames,
        })
    }
}

#[derive(Clone)]
struct Frame {
    delay: Duration,
    handle: image::Handle,
}

impl From<image_rs::Frame> for Frame {
    fn from(frame: image_rs::Frame) -> Self {
        let (width, height) = frame.buffer().dimensions();

        let delay = frame.delay().into();

        let handle = image::Handle::from_rgba(width, height, frame.into_buffer().into_vec());

        Self { delay, handle }
    }
}

struct State {
    index: usize,
    current: Current,
    total_bytes: u64,
}

struct Current {
    frame: Frame,
    started: Instant,
}

impl From<Frame> for Current {
    fn from(frame: Frame) -> Self {
        Self {
            started: Instant::now(),
            frame,
        }
    }
}

/// A frame that displays a GIF while keeping aspect ratio
#[derive(Debug)]
pub struct Gif<'a> {
    frames: &'a Frames,
    width: Length,
    height: Length,
    crop: Option<Rectangle<u32>>,
    border_radius: border::Radius,
    content_fit: ContentFit,
    filter_method: FilterMethod,
    rotation: Rotation,
    opacity: f32,
    scale: f32,
    expand: bool,
}

impl<'a> Gif<'a> {
    /// Creates a new [`Gif`] with the given [`Frames`]
    pub fn new(frames: &'a Frames) -> Self {
        Gif {
            frames,
            width: Length::Shrink,
            height: Length::Shrink,
            crop: None,
            border_radius: border::Radius::default(),
            content_fit: ContentFit::default(),
            filter_method: FilterMethod::default(),
            rotation: Rotation::default(),
            opacity: 1.0,
            scale: 1.0,
            expand: false,
        }
    }
    /// Sets the width of the [`Image`] boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`Image`] boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets whether the [`Image`] should try to fill as much space
    /// available as possible while keeping aspect ratio and without
    /// allocating extra space in any axis with a [`Length::Shrink`]
    /// sizing strategy.
    ///
    /// This is similar to using [`Length::Fill`] for both the
    /// [`width`](Self::width) and the [`height`](Self::height),
    /// but without the downside of blank space.
    pub fn expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    /// Sets the [`ContentFit`] of the [`Image`].
    ///
    /// Defaults to [`ContentFit::Contain`]
    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }

    /// Sets the [`FilterMethod`] of the [`Image`].
    pub fn filter_method(mut self, filter_method: FilterMethod) -> Self {
        self.filter_method = filter_method;
        self
    }

    /// Applies the given [`Rotation`] to the [`Image`].
    pub fn rotation(mut self, rotation: impl Into<Rotation>) -> Self {
        self.rotation = rotation.into();
        self
    }

    /// Sets the opacity of the [`Image`].
    ///
    /// It should be in the [0.0, 1.0] range—`0.0` meaning completely transparent,
    /// and `1.0` meaning completely opaque.
    pub fn opacity(mut self, opacity: impl Into<f32>) -> Self {
        self.opacity = opacity.into();
        self
    }

    /// Sets the scale of the [`Image`].
    ///
    /// The region of the [`Image`] drawn will be scaled from the center by the given scale factor.
    /// This can be useful to create certain effects and animations, like smooth zoom in / out.
    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.scale = scale.into();
        self
    }

    /// Crops the [`Image`] to the given region described by the [`Rectangle`] in absolute
    /// coordinates.
    ///
    /// Cropping is done before applying any transformation or [`ContentFit`]. In practice,
    /// this means that cropping an [`Image`] with this method should produce the same result
    /// as cropping it externally (e.g. with an image editor) and creating a new [`Handle`]
    /// for the cropped version.
    ///
    /// However, this method is much more efficient; since it just leverages scissoring during
    /// rendering and no image cropping actually takes place. Instead, it reuses the existing
    /// image allocations and should be as efficient as not cropping at all!
    ///
    /// The `region` coordinates will be clamped to the image dimensions, if necessary.
    pub fn crop(mut self, region: Rectangle<u32>) -> Self {
        self.crop = Some(region);
        self
    }

    /// Sets the [`border::Radius`] of the [`Image`].
    ///
    /// Currently, it will only be applied around the rectangular bounding box
    /// of the [`Image`].
    pub fn border_radius(mut self, border_radius: impl Into<border::Radius>) -> Self {
        self.border_radius = border_radius.into();
        self
    }
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer> for Gif<'a>
where
    Renderer: image::Renderer<Handle = Handle>,
{
    fn size(&self) -> Size<Length> {
        Size::new(self.width, self.height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State {
            index: 0,
            current: self.frames.first.clone().into(),
            total_bytes: self.frames.total_bytes,
        })
    }

    fn diff(&self, tree: &mut Tree) {
        let state = tree.state.downcast_mut::<State>();

        // Reset state if new gif Frames is used w/
        // same state tree.
        //
        // Total bytes of the gif should be a good enough
        // proxy for it changing.
        if state.total_bytes != self.frames.total_bytes {
            *state = State {
                index: 0,
                current: self.frames.first.clone().into(),
                total_bytes: self.frames.total_bytes,
            };
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        iced_widget::image::layout(
            renderer,
            limits,
            &self.frames.first.handle,
            self.width,
            self.height,
            self.crop,
            self.content_fit,
            self.rotation,
            self.expand,
        )
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let elapsed = now.duration_since(state.current.started);

            if elapsed > state.current.frame.delay {
                state.index = (state.index + 1) % self.frames.frames.len();

                state.current = self.frames.frames[state.index].clone().into();

                shell.request_redraw_at(*now + state.current.frame.delay);
            } else {
                let remaining = state.current.frame.delay - elapsed;

                shell.request_redraw_at(*now + remaining);
            }
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();

        iced_widget::image::draw(
            renderer,
            layout,
            &state.current.frame.handle,
            self.crop,
            self.border_radius,
            self.content_fit,
            self.filter_method,
            self.rotation,
            self.opacity,
            self.scale,
        );
    }
}

impl<'a, Message, Theme, Renderer> From<Gif<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: image::Renderer<Handle = Handle> + 'a,
{
    fn from(gif: Gif<'a>) -> Element<'a, Message, Theme, Renderer> {
        Element::new(gif)
    }
}
