//! Local image conversion and FFmpeg-backed audio/video conversion.
pub mod batch;
pub mod control;
pub mod images;
pub mod media;
mod output;

use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Image(#[from] image::ImageError),
    #[error("{program} failed: {message}")]
    Process { program: String, message: String },
}
pub type Result<T> = std::result::Result<T, Error>;

pub fn extension(path: &Path) -> Result<String> {
    path.extension()
        .and_then(|s| s.to_str())
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| Error::Invalid(format!("Missing file extension: {}", path.display())))
}
