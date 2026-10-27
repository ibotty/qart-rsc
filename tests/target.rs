use qart::target::Target;

#[test]
fn loads_pjw() {
    let img = image::open(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/pjw.png")).unwrap();
    let t = Target::from_image(&img, 41);
    assert_eq!(t.width().max(t.height()), 41);
    assert!(t.width() > 0 && t.height() > 0);
    let opaque = (0..t.height() as i64)
        .flat_map(|y| (0..t.width() as i64).map(move |x| (x, y)))
        .filter(|&(x, y)| t.get(x, y).is_some())
        .count();
    assert!(opaque > 0);
}
