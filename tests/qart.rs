use image::DynamicImage;
use qart::{Code, Mask, Options, Qart, Rotation, Version, encode};

const URL: &str = "https://research.swtch.com/qart";

fn pjw() -> DynamicImage {
    image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pjw.png")).unwrap()
}

fn options(version: u8, url: &str) -> Options {
    Options::new(url).version(version).unwrap()
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
    encode(&pjw(), &opts.clone().seed(seed)).unwrap()
}

#[test]
fn decodes_to_url_and_digits() {
    let q = run(&options(6, URL), 1);
    assert_eq!(decode(q.code()), q.text());
    let digits = q
        .text()
        .strip_prefix("https://research.swtch.com/qart#")
        .unwrap();
    assert!(digits.len() > 100);
    assert!(digits.bytes().all(|c| c.is_ascii_digit()));
}

#[test]
fn deterministic_for_a_seed() {
    let opts = options(6, URL);
    let (a, b) = (run(&opts, 7), run(&opts, 7));
    assert_eq!(a.code(), b.code());
    assert_eq!(a.control_image(1, 0), b.control_image(1, 0));
    assert_eq!(a.text(), b.text());
    let c = run(&opts, 8);
    assert_eq!(decode(c.code()), c.text());
    assert!(c.text().starts_with("https://research.swtch.com/qart#"));
}

#[test]
fn all_versions_decode() {
    for v in 1..=8 {
        let q = run(&options(v, "https://a.b/"), u64::from(v));
        assert_eq!(decode(q.code()), q.text(), "version {v}");
    }
}

#[test]
fn all_rotations_decode() {
    for rotation in [Rotation::R0, Rotation::R90, Rotation::R180, Rotation::R270] {
        let opts = options(6, URL).rotation(rotation);
        let q = run(&opts, 3);
        assert_eq!(decode(q.code()), q.text(), "{rotation:?}");
    }
}

#[test]
fn url_too_long_for_version() {
    let opts = options(1, "https://example.com/0123456789012345678901234567890");
    let err = encode(&pjw(), &opts).unwrap_err();
    assert!(matches!(err, qart::Error::UrlTooLong));
}

#[test]
fn rand_decodes_and_depends_on_seed() {
    let opts = options(6, URL).rand(true);
    let (a, b) = (run(&opts, 1), run(&opts, 2));
    assert_eq!(decode(a.code()), a.text());
    assert_eq!(decode(b.code()), b.text());
    assert!(a.code() != b.code());
}

#[test]
fn control_image_marks_controlled_modules() {
    let q = run(&options(6, URL), 1);
    let (scale, border) = (3, 2);
    let img = q.control_image(scale, border);
    let side = (q.size() as u32 + 2 * border) * scale;
    assert_eq!(img.dimensions(), (side, side));
    let size = q.size();
    let mut seen = [false; 2];
    for y in 0..size {
        for x in 0..size {
            let px = img.get_pixel((x as u32 + border) * scale, (y as u32 + border) * scale);
            if q.controlled(x, y) {
                let black = q.code().black(x, y);
                assert_eq!(px.0, if black { [0, 0, 0, 255] } else { [255; 4] });
                seen[usize::from(black)] = true;
            } else {
                assert_eq!(
                    px.0,
                    if q.code().black(x, y) {
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
    assert_eq!(q.to_image(1, 0).dimensions(), (21, 21));
    assert_eq!(q.to_image(4, 4).dimensions(), (116, 116));
    let img = q.to_image(2, 1);
    assert_eq!(img.get_pixel(0, 0).0, [255]);
    assert_eq!(img.get_pixel(2, 2).0, [0]);
}

#[test]
fn modules_cover_the_code_row_by_row() {
    let q = run(&options(1, "https://a.b/"), 1);
    let size = q.size();
    let modules: Vec<_> = q.modules().collect();
    assert_eq!(modules.len(), size * size);
    for (i, m) in modules.iter().enumerate() {
        assert_eq!((m.x, m.y), (i % size, i / size));
        assert_eq!(m.black, q.code().black(m.x, m.y));
        assert_eq!(m.controlled, q.controlled(m.x, m.y));
    }
}

#[test]
fn query_param_places_digits_before_the_fragment() {
    let cases = [
        ("https://e.com/a", "https://e.com/a?q=", ""),
        ("https://e.com/a?", "https://e.com/a?q=", ""),
        ("https://e.com/a?x=1", "https://e.com/a?x=1&q=", ""),
        (
            "https://e.com/a#an-anchor",
            "https://e.com/a?q=",
            "#an-anchor",
        ),
        ("https://e.com/a?x=1#top", "https://e.com/a?x=1&q=", "#top"),
        ("https://e.com/a#", "https://e.com/a?q=", "#"),
    ];
    for (url, prefix, suffix) in cases {
        let opts = options(6, url).query_param("q").unwrap();
        let q = run(&opts, 1);
        assert_eq!(decode(q.code()), q.text(), "{url}");
        let digits = q
            .text()
            .strip_prefix(prefix)
            .and_then(|t| t.strip_suffix(suffix))
            .unwrap_or_else(|| panic!("{url}: {}", q.text()));
        assert!(digits.len() > 100, "{url}");
        assert!(digits.bytes().all(|c| c.is_ascii_digit()), "{url}");
    }
}

#[test]
fn fragment_needs_query_param() {
    let opts = options(6, "https://e.com/a#top");
    let err = encode(&pjw(), &opts).unwrap_err();
    assert!(matches!(err, qart::Error::FragmentInUrl));
}

#[test]
fn query_param_is_validated() {
    for key in ["", "a=b", "a&b", "a#b", "a?b", "a b", "é"] {
        let err = Options::new(URL).query_param(key).unwrap_err();
        assert!(
            matches!(err, qart::Error::InvalidParam(k) if k == key),
            "{key:?}"
        );
    }
    assert!(Options::new(URL).query_param("a-B_c.d~1").is_ok());
}

#[test]
fn version_and_mask_take_numbers_or_types() {
    let a = Options::new(URL).version(6).unwrap().mask(2).unwrap();
    let b = Options::new(URL)
        .version(Version::try_from(6).unwrap())
        .unwrap()
        .mask(Mask::try_from(2).unwrap())
        .unwrap();
    assert_eq!(run(&a, 1).text(), run(&b, 1).text());
    assert_eq!(run(&a, 1).code(), run(&b, 1).code());
}

#[test]
fn version_and_mask_are_validated() {
    for v in [0, 41] {
        let err = Options::new(URL).version(v).unwrap_err();
        assert!(matches!(err, qart::Error::InvalidVersion(n) if n == v));
    }
    let err = Options::new(URL).mask(8).unwrap_err();
    assert!(matches!(err, qart::Error::InvalidMask(8)));
}
