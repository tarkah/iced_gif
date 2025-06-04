//! Display a GIF in your user interface
use std::fmt;
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};

#[allow(unused)]
use iced_widget::core::image::Image;
use iced_widget::core::image::{self, FilterMethod, Handle};
use iced_widget::core::mouse::Cursor;
use iced_widget::core::widget::{tree, Tree};
use iced_widget::core::{
    layout, renderer, window, Clipboard, ContentFit, Element, Event, Layout, Length, Point,
    Rectangle, Rotation, Shell, Size, Vector, Widget,
};
use iced_widget::runtime::Task;
use image_rs::codecs::gif;
use image_rs::{AnimationDecoder, ImageDecoder};

#[cfg(not(feature = "tokio"))]
use iced_futures::futures::{AsyncRead, AsyncReadExt};
#[cfg(feature = "tokio")]
use tokio::io::{AsyncRead, AsyncReadExt};

/// Error loading or decoding a gif
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Decode error
    #[error(transparent)]
    Image(#[from] image_rs::ImageError),
    /// Load error
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// The frames of a decoded gif
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

        Self::from_bytes(bytes)
    }

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
    content_fit: ContentFit,
    filter_method: FilterMethod,
    rotation: Rotation,
    opacity: f32,
}

impl<'a> Gif<'a> {
    /// Creates a new [`Gif`] with the given [`Frames`]
    pub fn new(frames: &'a Frames) -> Self {
        Gif {
            frames,
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::default(),
            filter_method: FilterMethod::default(),
            rotation: Rotation::default(),
            opacity: 1.0,
        }
    }

    /// Sets the width of the [`Gif`] boundaries.
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// Sets the height of the [`Gif`] boundaries.
    pub fn height(mut self, height: Length) -> Self {
        self.height = height;
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
        &self,
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
            self.content_fit,
            self.rotation,
            false,
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

                shell
                    .request_redraw_at(window::RedrawRequest::At(*now + state.current.frame.delay));
            } else {
                let remaining = state.current.frame.delay - elapsed;

                shell.request_redraw_at(window::RedrawRequest::At(*now + remaining));
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

        // Pulled from iced_native::widget::<Image as Widget>::draw
        //
        // TODO: export iced_native::widget::image::draw as standalone function
        {
            let Size { width, height } = renderer.measure_image(&state.current.frame.handle);
            let image_size = Size::new(width as f32, height as f32);
            let rotated_size = self.rotation.apply(image_size);

            let bounds = layout.bounds();
            let adjusted_fit = self.content_fit.fit(rotated_size, bounds.size());

            let scale = Vector::new(
                adjusted_fit.width / rotated_size.width,
                adjusted_fit.height / rotated_size.height,
            );

            let final_size = image_size * scale;

            let position = match self.content_fit {
                ContentFit::None => Point::new(
                    bounds.x + (rotated_size.width - adjusted_fit.width) / 2.0,
                    bounds.y + (rotated_size.height - adjusted_fit.height) / 2.0,
                ),
                _ => Point::new(
                    bounds.center_x() - final_size.width / 2.0,
                    bounds.center_y() - final_size.height / 2.0,
                ),
            };

            let drawing_bounds = Rectangle::new(position, final_size);

            let render = |renderer: &mut Renderer| {
                renderer.draw_image(
                    image::Image {
                        handle: state.current.frame.handle.clone(),
                        filter_method: self.filter_method,
                        rotation: self.rotation.radians(),
                        opacity: self.opacity,
                        snap: true,
                    },
                    drawing_bounds,
                );
            };

            if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
                renderer.with_layer(bounds, render);
            } else {
                render(renderer);
            }
        }
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
