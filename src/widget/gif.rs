use std::fmt;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::{Duration, Instant};

use ::image::codecs::gif;
use ::image::{AnimationDecoder, ImageDecoder};
use iced_futures::MaybeSend;
use iced_native::image::{self, Handle};
use iced_native::widget::{tree, Tree};
use iced_native::{
    event, layout, renderer, window, Clipboard, Command, ContentFit, Element, Event, Layout,
    Length, Point, Rectangle, Shell, Size, Vector, Widget,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Image(#[from] ::image::ImageError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

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
    pub fn from_reader<R: Read>(reader: R) -> Result<Self, Error> {
        let decoder = gif::GifDecoder::new(reader)?;

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

    pub fn load_from_path<Message>(
        path: impl AsRef<Path>,
        on_load: impl FnOnce(Result<Frames, Error>) -> Message + 'static + MaybeSend,
    ) -> Command<Message> {
        let path = path.as_ref().to_path_buf();

        let f = async move {
            let reader = BufReader::new(File::open(path)?);

            Self::from_reader(reader)
        };

        Command::perform(f, on_load)
    }
}

#[derive(Clone)]
struct Frame {
    delay: Duration,
    handle: image::Handle,
}

impl From<::image::Frame> for Frame {
    fn from(frame: ::image::Frame) -> Self {
        let (width, height) = frame.buffer().dimensions();

        let delay = frame.delay().into();

        let handle = image::Handle::from_pixels(width, height, frame.into_buffer().into_vec());

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

#[derive(Debug)]
pub struct Gif<'a> {
    frames: &'a Frames,
    width: Length,
    height: Length,
    content_fit: ContentFit,
}

impl<'a> Gif<'a> {
    /// Creates a new [`Gif`] with the given [`Frames`]
    pub fn new(frames: &'a Frames) -> Self {
        Gif {
            frames,
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::Contain,
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

    /// Sets the [`ContentFit`] of the [`Gif`].
    ///
    /// Defaults to [`ContentFit::Contain`]
    pub fn content_fit(self, content_fit: ContentFit) -> Self {
        Self {
            content_fit,
            ..self
        }
    }
}

impl<'a, Message, Renderer> Widget<Message, Renderer> for Gif<'a>
where
    Renderer: image::Renderer<Handle = Handle>,
{
    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
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

    fn layout(&self, renderer: &Renderer, limits: &layout::Limits) -> layout::Node {
        iced_native::widget::image::layout(
            renderer,
            limits,
            &self.frames.first.handle,
            self.width,
            self.height,
            self.content_fit,
        )
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        _layout: Layout<'_>,
        _cursor_position: Point,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        let state = tree.state.downcast_mut::<State>();

        if let Event::Window(window::Event::RedrawRequested(now)) = event {
            let elapsed = now.duration_since(state.current.started);

            if elapsed > state.current.frame.delay {
                state.index = (state.index + 1) % self.frames.frames.len();

                state.current = self.frames.frames[state.index].clone().into();

                shell.request_redraw(window::RedrawRequest::At(now + state.current.frame.delay));
            } else {
                let remaining = state.current.frame.delay - elapsed;

                shell.request_redraw(window::RedrawRequest::At(now + remaining));
            }
        }

        event::Status::Ignored
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        _theme: &Renderer::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor_position: Point,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();

        // Pulled from iced_native::widget::<Image as Widget>::draw
        //
        // TODO: export iced_native::widget::image::draw as standalone function
        {
            let Size { width, height } = renderer.dimensions(&state.current.frame.handle);
            let image_size = Size::new(width as f32, height as f32);

            let bounds = layout.bounds();
            let adjusted_fit = self.content_fit.fit(image_size, bounds.size());

            let render = |renderer: &mut Renderer| {
                let offset = Vector::new(
                    (bounds.width - adjusted_fit.width).max(0.0) / 2.0,
                    (bounds.height - adjusted_fit.height).max(0.0) / 2.0,
                );

                let drawing_bounds = Rectangle {
                    width: adjusted_fit.width,
                    height: adjusted_fit.height,
                    ..bounds
                };

                renderer.draw(state.current.frame.handle.clone(), drawing_bounds + offset)
            };

            if adjusted_fit.width > bounds.width || adjusted_fit.height > bounds.height {
                renderer.with_layer(bounds, render);
            } else {
                render(renderer)
            }
        }
    }
}

impl<'a, Message, Renderer> From<Gif<'a>> for Element<'a, Message, Renderer>
where
    Renderer: image::Renderer<Handle = Handle> + 'a,
{
    fn from(gif: Gif<'a>) -> Element<'a, Message, Renderer> {
        Element::new(gif)
    }
}
