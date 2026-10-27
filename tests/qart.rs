use image::DynamicImage;
use qart::art::{Options, Qart, encode};
use qart::coding::{Code, Level, Mask, Plan, Role, Rotation, Version};
use qart::render::control_image;
use qart::target::Target;
use rand::SeedableRng;
use rand::rngs::StdRng;

const URL: &str = "https://research.swtch.com/qart";

fn pjw() -> DynamicImage {
    image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pjw.png")).unwrap()
}

fn options(version: u8, url: &str) -> Options {
    Options {
        dx: 4,
        dy: 4,
        ..Options::new(url, Version::new(version).unwrap(), Mask::new(2).unwrap())
    }
}

fn decode(code: &Code) -> String {
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

#[test]
fn rand_decodes_and_depends_on_seed() {
    let opts = Options {
        rand: true,
        ..options(6, URL)
    };
    let (a, b) = (run(&opts, 1), run(&opts, 2));
    assert_eq!(decode(&a.code), a.text);
    assert_eq!(decode(&b.code), b.text);
    assert!(a.code != b.code);
}

#[test]
fn only_data_bits_leaves_check_modules() {
    let opts = Options {
        only_data_bits: true,
        ..options(6, URL)
    };
    let q = run(&opts, 1);
    assert_eq!(decode(&q.code), q.text);
    let plan = Plan::new(opts.version, Level::L, opts.mask);
    let size = q.code.size();
    let mut data = 0;
    for y in 0..size {
        for x in 0..size {
            if q.controlled[y * size + x] {
                assert_eq!(plan.pixel(x, y).role, Role::Data, "({x}, {y})");
                data += 1;
            }
        }
    }
    assert!(data > 0);
}

#[test]
fn control_image_marks_controlled_modules() {
    let q = run(&options(6, URL), 1);
    let (scale, border) = (3, 2);
    let img = control_image(&q, scale, border);
    let side = (q.code.size() as u32 + 2 * border) * scale;
    assert_eq!(img.dimensions(), (side, side));
    let size = q.code.size();
    let mut seen = [false; 2];
    for y in 0..size {
        for x in 0..size {
            let px = img.get_pixel((x as u32 + border) * scale, (y as u32 + border) * scale);
            if q.controlled[y * size + x] {
                let black = q.code.black(x, y);
                assert_eq!(px.0, if black { [0, 0, 0, 255] } else { [255; 4] });
                seen[usize::from(black)] = true;
            } else {
                assert_eq!(
                    px.0,
                    if q.code.black(x, y) {
                        [0x3f, 0x3f, 0x3f, 255]
                    } else {
                        [0xbf, 0xbf, 0xbf, 255]
                    }
                );
            }
        }
    }
    assert_eq!(seen, [true; 2]);
}

#[test]
fn to_image_dimensions() {
    let q = run(&options(1, "https://a.b/"), 1);
    assert_eq!(q.code.to_image(1, 0).dimensions(), (21, 21));
    assert_eq!(q.code.to_image(4, 4).dimensions(), (116, 116));
    let img = q.code.to_image(2, 1);
    assert_eq!(img.get_pixel(0, 0).0, [255]);
    assert_eq!(img.get_pixel(2, 2).0, [0]);
}
