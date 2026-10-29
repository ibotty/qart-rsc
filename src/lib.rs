//! Embeds an image in a QR code that still scans. Rust port of the `qart`
//! package of [rsc/qr](https://github.com/rsc/qr), described in Russ Cox's
//! [QR Codes as Art](https://research.swtch.com/qart).
//!
//! The code decodes to the URL, a `#` and a run of digits; [`encode`] chooses
//! the digits so that the modules resemble the image.
//!
#![doc = include_str!("../docs/example.svg")]
//!
//! ```no_run
//! use qart::{Options, encode};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let img = image::open("tests/pjw.png")?;
//! let opts = Options::new("https://research.swtch.com/qart");
//! let q = encode(&img, &opts)?;
//! q.to_image(8, 4).save("out.png")?;
//! q.control_image(8, 4).save("control.png")?;
//! println!("{}", q.text());
//! # Ok(())
//! # }
//! ```
//!
//! [`Qart::modules`] iterates over the modules, for other output formats.

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
    #[error("invalid query parameter name {0:?}")]
    InvalidParam(String),
    #[error("URL has a fragment; keep it with Options::query_param")]
    FragmentInUrl,
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

impl From<std::convert::Infallible> for Error {
    fn from(e: std::convert::Infallible) -> Self {
        match e {}
    }
}

pub type Result<T> = std::result::Result<T, Error>;
