pub mod art;
pub mod coding;
pub mod gf256;
pub mod render;
pub mod target;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid QR version {0}")]
    InvalidVersion(u8),
    #[error("invalid QR mask {0}")]
    InvalidMask(u8),
    #[error("invalid rotation of {0} quarter turns")]
    InvalidRotation(u8),
    #[error("cannot encode {have} bits into {want}-bit code")]
    TooLong { have: usize, want: usize },
    #[error("cannot encode URL into available bits")]
    UrlTooLong,
    #[error("cannot preserve required bits")]
    CannotPreserveBits,
    #[error("non-numeric string {0:?}")]
    NonNumeric(String),
    #[error(transparent)]
    Image(#[from] image::ImageError),
}

pub type Result<T> = std::result::Result<T, Error>;
