use image::{DynamicImage, GrayImage, Luma};
use qart::art::{Options, Qart, encode};
use qart::coding::{Code, Mask, Rotation, Version};
use qart::target::Target;
use rand::SeedableRng;
use rand::rngs::StdRng;

const URL: &str = "https://research.swtch.com/qart";

fn pjw() -> DynamicImage {
    image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pjw.png")).unwrap()
}

fn options(version: u8, url: &str) -> Options {
    Options {
        url: url.to_owned(),
        version: Version::new(version).unwrap(),
        mask: Mask::new(2).unwrap(),
        dx: 4,
        dy: 4,
        size: 0,
        rotation: Rotation::R0,
    }
}

fn render(code: &Code, scale: u32, border: u32) -> GrayImage {
    let side = (code.size() as u32 + 2 * border) * scale;
    GrayImage::from_fn(side, side, |x, y| {
        let (mx, my) = (x / scale, y / scale);
        let black = mx >= border
            && my >= border
            && code.black((mx - border) as usize, (my - border) as usize);
        Luma([if black { 0 } else { 255 }])
    })
}

fn decode(code: &Code) -> String {
    let img = render(code, 8, 4);
    let mut prepared = rqrr::PreparedImage::prepare_from_greyscale(
        img.width() as usize,
        img.height() as usize,
        |x, y| img.get_pixel(x as u32, y as u32).0[0],
    );
    let grids = prepared.detect_grids();
    assert_eq!(grids.len(), 1, "expected one QR code");
    grids[0].decode().expect("code decodes").1
}

fn run(opts: &Options, seed: u64) -> Qart {
    encode(&pjw(), opts, &mut StdRng::seed_from_u64(seed)).unwrap()
}

#[test]
fn decodes_to_url_and_digits() {
    let q = run(&options(6, URL), 1);
    assert_eq!(decode(&q.code), q.text);
    let digits = q
        .text
        .strip_prefix("https://research.swtch.com/qart#")
        .unwrap();
    assert!(digits.len() > 100);
    assert!(digits.bytes().all(|c| c.is_ascii_digit()));
}

#[test]
fn deterministic_for_a_seed() {
    let opts = options(6, URL);
    let (a, b) = (run(&opts, 7), run(&opts, 7));
    assert_eq!(a.code, b.code);
    assert_eq!(a.controlled, b.controlled);
    assert_eq!(a.text, b.text);
    let c = run(&opts, 8);
    assert_eq!(decode(&c.code), c.text);
    assert!(c.text.starts_with("https://research.swtch.com/qart#"));
}

#[test]
fn all_versions_decode() {
    for v in 1..=8 {
        let q = run(&options(v, "https://a.b/"), u64::from(v));
        assert_eq!(decode(&q.code), q.text, "version {v}");
    }
}

#[test]
fn all_rotations_decode() {
    for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
        let opts = Options {
            rotation,
            ..options(6, URL)
        };
        let q = run(&opts, 3);
        assert_eq!(decode(&q.code), q.text, "{rotation:?}");
    }
}

#[test]
fn controlled_modules_follow_the_image() {
    let opts = options(6, URL);
    let q = run(&opts, 1);
    let size = q.code.size();
    let target = Target::from_image(&pjw(), (size as i64 + opts.size) as u32);

    let (mut controlled, mut matching) = (0, 0);
    for y in 0..size {
        for x in 0..size {
            if q.controlled[y * size + x] {
                controlled += 1;
                let (targ, _) = target.sample(x, y, opts.dx, opts.dy);
                matching += usize::from((targ < 128) == q.code.black(x, y));
            }
        }
    }
    eprintln!(
        "controlled {controlled}/{} matching {matching}",
        size * size
    );
    assert!(controlled > size * size / 3);
    assert!(matching * 100 >= controlled * 99);
}

#[test]
fn url_too_long_for_version() {
    let opts = options(1, "https://example.com/0123456789012345678901234567890");
    let err = encode(&pjw(), &opts, &mut StdRng::seed_from_u64(1)).unwrap_err();
    assert!(matches!(err, qart::Error::UrlTooLong));
}
