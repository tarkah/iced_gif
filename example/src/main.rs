use std::path::PathBuf;

use iced::widget::{container, row};
use iced::{window, Element, Length, Size, Task};
use iced_gif::widget::gif;

fn main() -> iced::Result {
    iced::application(App::new, App::update, App::view)
        .title(App::title)
        .window(window::Settings {
            size: Size::new(498.0, 164.0),
            ..Default::default()
        })
        .run()
}

#[derive(Debug)]
enum Message {
    Loaded(Result<gif::Frames, gif::Error>),
}

#[derive(Default)]
struct App {
    frames: Option<gif::Frames>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/rust-lang-ferris.gif");

        (
            App::default(),
            gif::Frames::load_from_path(path).map(Message::Loaded),
        )
    }

    fn title(&self) -> String {
        "Iced Gif".into()
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        let Message::Loaded(frames) = message;

        self.frames = frames.ok();

        Task::none()
    }

    fn view(&self) -> Element<Message> {
        if let Some(frames) = self.frames.as_ref() {
            container(gif(frames))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        } else {
            row![].into()
        }
    }
}
