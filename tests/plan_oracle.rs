//! Compares the planned codes against the `qrcodegen` crate.

use qart::coding::{Level, Mask, Plan, Segment, Version};
use qrcodegen::{QrCode, QrCodeEcc, QrSegment};

fn oracle_level(l: Level) -> QrCodeEcc {
    match l {
        Level::L => QrCodeEcc::Low,
        Level::M => QrCodeEcc::Medium,
        Level::Q => QrCodeEcc::Quartile,
        Level::H => QrCodeEcc::High,
    }
}

#[test]
fn matches_qrcodegen() {
    let inputs: [(Segment, QrSegment); 4] = [
        (Segment::Bytes(b"hello"), QrSegment::make_bytes(b"hello")),
        (Segment::Numeric("1"), QrSegment::make_numeric("1")),
        (Segment::Numeric("12"), QrSegment::make_numeric("12")),
        (Segment::Numeric("123"), QrSegment::make_numeric("123")),
    ];
    for v in 1..=40 {
        let version = Version::new(v).unwrap();
        for level in [Level::L, Level::M, Level::Q, Level::H] {
            for m in 0..8 {
                let mask = Mask::new(m).unwrap();
                let plan = Plan::new(version, level, mask);
                for (seg, oseg) in &inputs {
                    let ov = qrcodegen::Version::new(v);
                    let want = QrCode::encode_segments_advanced(
                        std::slice::from_ref(oseg),
                        oracle_level(level),
                        ov,
                        ov,
                        Some(qrcodegen::Mask::new(m)),
                        false,
                    )
                    .unwrap();
                    let got = plan.encode(&[*seg]).unwrap();
                    assert_eq!(got.size() as i32, want.size());
                    for y in 0..got.size() {
                        for x in 0..got.size() {
                            assert_eq!(
                                got.black(x, y),
                                want.get_module(x as i32, y as i32),
                                "v{v} {level:?} mask {m} {seg:?} at ({x}, {y})"
                            );
                        }
                    }
                }
            }
        }
    }
}
