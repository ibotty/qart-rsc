mod art;
mod coding;
mod gf256;
mod render;
mod target;

pub use art::{Module, Options, Qart, encode};
pub use coding::{Code, Mask, Rotation, Version};

#[cfg(test)]
pub(crate) mod testutil {
    use image::DynamicImage;

    use crate::Code;

    pub fn pjw() -> DynamicImage {
        image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pjw.png")).unwrap()
    }

    pub fn decode(code: &Code) -> String {
        let img = code.to_image(8, 4);
        let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(
            img.width() as usize,
            img.height() as usize,
            |x, y| img.get_pixel(x as u32, y as u32).0[0],
        );
        let grids = prepared.detect_grids();
        assert_eq!(grids.len(), 1, "expected one QR code");
        grids[0].decode().expect("code decodes").1
    }
}

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
