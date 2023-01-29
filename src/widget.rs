pub mod gif;

pub use gif::Gif;

/// Creates a new [`Gif`] with the given [`gif::Frames`]
pub fn gif(frames: &gif::Frames) -> Gif {
    Gif::new(frames)
}
