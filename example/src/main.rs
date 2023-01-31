use std::path::PathBuf;

use iced::widget::{container, row};
use iced::{
    application, color, executor, theme, window, Application, Command, Element, Length, Settings,
    Theme,
};
use iced_gif::widget::gif;

fn main() {
    let settings = Settings {
        window: window::Settings {
            size: (498, 164),
            ..Default::default()
        },
        ..Default::default()
    };

    App::run(settings).unwrap()
}

#[derive(Debug)]
enum Message {
    Loaded(Result<gif::Frames, gif::Error>),
}

#[derive(Default)]
struct App {
    frames: Option<gif::Frames>,
}

impl Application for App {
    type Executor = executor::Default;
    type Message = Message;
    type Theme = Theme;
    type Flags = ();

    fn new(_flags: Self::Flags) -> (Self, Command<Message>) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../assets/rust-lang-ferris.gif");

        (
            App::default(),
            gif::Frames::load_from_path_async(path).map(Message::Loaded),
        )
    }

    fn title(&self) -> String {
        "Iced Gif".into()
    }

    fn style(&self) -> theme::Application {
        theme::Application::Custom(Box::new(Style))
    }

    fn update(&mut self, message: Message) -> Command<Message> {
        let Message::Loaded(frames) = message;

        self.frames = frames.ok();

        Command::none()
    }

    fn view(&self) -> Element<Message> {
        if let Some(frames) = self.frames.as_ref() {
            container(gif(frames))
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x()
                .center_y()
                .into()
        } else {
            row![].into()
        }
    }
}

pub struct Style;

impl application::StyleSheet for Style {
    type Style = Theme;

    fn appearance(&self, style: &Self::Style) -> application::Appearance {
        application::Appearance {
            background_color: color!(0xFCFEFC),
            text_color: style.palette().text,
        }
    }
}
