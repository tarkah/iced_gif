pub mod widget;

pub use widget::gif;
pub use widget::gif::{Frames, Gif};

use std::sync::Arc;

/// Error loading or decoding a gif
#[derive(thiserror::Error, Debug, Clone)]
pub enum Error {
    /// Decode error
    #[error(transparent)]
    Image(Arc<image_rs::ImageError>),
    /// Load error
    #[error(transparent)]
    Io(Arc<std::io::Error>),
    /// Networking error
    #[cfg(feature = "networking")]
    #[error(transparent)]
    Networking(Arc<reqwest::Error>),
}

impl From<image_rs::ImageError> for Error {
    fn from(value: image_rs::ImageError) -> Self {
        Error::Image(Arc::new(value))
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Error::Io(Arc::new(value))
    }
}

#[cfg(feature = "networking")]
impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Error::Networking(Arc::new(value))
    }
}
