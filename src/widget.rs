pub mod gif;

pub use gif::Gif;

pub fn gif(frames: &gif::Frames) -> Gif {
    Gif::new(frames)
}
