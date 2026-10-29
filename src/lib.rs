//! Embeds an image in a QR code that still scans. Rust port of the `qart`
//! package of [rsc/qr](https://github.com/rsc/qr), described in Russ Cox's
//! [QR Codes as Art](https://research.swtch.com/qart).
//!
#![doc = include_str!("../docs/example.svg")]
//!
//! ```no_run
//! use qart_rsc::{Qart, image};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let img = image::open("tests/pjw.png")?;
//! let q = Qart::from_image(&img)
//!     .version(5)?
//!     .encode("https://research.swtch.com/qart")?;
//! q.to_image(8, 4).save("out.png")?;
//! q.control_image(8, 4).save("control.png")?;
//! println!("{}", q.text());
//! # Ok(())
//! # }
//! ```
//!
//! [`Qart::from_image`] starts an [`Encoder`], and [`Encoder::encode`] makes
//! the [`Qart`]. The code decodes to the URL, a `#` and a run of digits that
//! `encode` chooses so that the modules resemble the image. A URL that
//! already has a fragment is rejected, unless [`Encoder::query_param`] puts
//! the digits in a query parameter before the fragment, as in
//! `https://example.com/?k=<digits>#top`.
//!
//! [`Qart::modules`] iterates over the modules, for other output formats.
//!
//! The `image` crate is re-exported.

mod art;
mod coding;
mod gf256;
mod render;
mod target;

pub use art::{Encoder, Module, Qart};
pub use image;

#[cfg(test)]
pub(crate) mod testutil {
    use image::DynamicImage;

    pub fn pjw() -> DynamicImage {
        image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pjw.png")).unwrap()
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
    #[error("cannot encode URL into available bits")]
    UrlTooLong,
}

pub type Result<T> = std::result::Result<T, Error>;
