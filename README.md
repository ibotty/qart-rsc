# qart

Embeds an image in a QR code that still scans. Rust port of the `qart`
package of [rsc/qr](https://github.com/rsc/qr), described in Russ Cox's
[QR Codes as Art](https://research.swtch.com/qart).

```rust
use qart::{Options, encode};

let img = image::open("tests/pjw.png")?;
let opts = Options { dx: 4, dy: 4, ..Options::new("https://research.swtch.com/qart") };
let q = encode(&img, &opts)?;
q.to_image(8, 4).save("out.png")?;
q.control_image(8, 4).save("control.png")?;
println!("{}", q.text());
```

Command line:

```sh
cargo run --example qart -- tests/pjw.png https://research.swtch.com/qart -o out.png
```

## Credits

Based on [rsc/qr](https://github.com/rsc/qr) by Russ Cox. Licensed under the
same BSD-3-Clause license as the original, see [LICENSE](LICENSE).
`tests/pjw.png` is from the original repository.
