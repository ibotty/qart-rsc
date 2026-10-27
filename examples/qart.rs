use std::process::ExitCode;
use std::str::FromStr;

use qart::art::{Options, encode};
use qart::coding::{Mask, Rotation, Version};
use qart::render::control_image;

const USAGE: &str = "usage: qart <image> <url> [-o out.png] [--version N] [--mask N] \
[--dx N] [--dy N] [--size N] [--rot 0..3] [--seed N] [--rand] [--only-data] \
[--control control.png] [--scale N]";

struct Args {
    image: String,
    url: String,
    out: String,
    control: Option<String>,
    version: u8,
    mask: u8,
    dx: i64,
    dy: i64,
    size: i64,
    rot: u8,
    seed: Option<u64>,
    rand: bool,
    only_data: bool,
    scale: u32,
}

fn parse<T: FromStr>(flag: &str, value: Option<String>) -> Result<T, String> {
    let value = value.ok_or(format!("{flag} needs a value"))?;
    value
        .parse()
        .map_err(|_| format!("invalid {flag}: {value}"))
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        image: String::new(),
        url: String::new(),
        out: "out.png".into(),
        control: None,
        version: 6,
        mask: 2,
        dx: 4,
        dy: 4,
        size: 0,
        rot: 0,
        seed: None,
        rand: false,
        only_data: false,
        scale: 8,
    };
    let mut positional = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-o" => args.out = parse(&arg, it.next())?,
            "--control" => args.control = Some(parse(&arg, it.next())?),
            "--version" => args.version = parse(&arg, it.next())?,
            "--mask" => args.mask = parse(&arg, it.next())?,
            "--dx" => args.dx = parse(&arg, it.next())?,
            "--dy" => args.dy = parse(&arg, it.next())?,
            "--size" => args.size = parse(&arg, it.next())?,
            "--rot" => args.rot = parse(&arg, it.next())?,
            "--seed" => args.seed = Some(parse(&arg, it.next())?),
            "--scale" => args.scale = parse(&arg, it.next())?,
            "--rand" => args.rand = true,
            "--only-data" => args.only_data = true,
            "-h" | "--help" => return Err(String::new()),
            _ if arg.starts_with('-') => return Err(format!("unknown flag {arg}")),
            _ => positional.push(arg),
        }
    }
    let [image, url] = <[String; 2]>::try_from(positional)
        .map_err(|_| "expected an image and a URL".to_owned())?;
    args.image = image;
    args.url = url;
    Ok(args)
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let rotation = match args.rot {
        0 => Rotation::R0,
        1 => Rotation::R90,
        2 => Rotation::R180,
        3 => Rotation::R270,
        n => return Err(format!("invalid --rot: {n}").into()),
    };
    let opts = Options {
        dx: args.dx,
        dy: args.dy,
        size: args.size,
        rotation,
        rand: args.rand,
        only_data_bits: args.only_data,
        seed: args.seed,
        ..Options::new(
            args.url,
            Version::new(args.version)?,
            Mask::new(args.mask).ok_or("invalid --mask")?,
        )
    };
    let img = image::open(&args.image)?;
    let q = encode(&img, &opts)?;
    q.code.to_image(args.scale, 4).save(&args.out)?;
    if let Some(path) = &args.control {
        control_image(&q, args.scale, 4).save(path)?;
    }
    println!("{}", q.text);
    Ok(())
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(args) => args,
        Err(msg) => {
            if !msg.is_empty() {
                eprintln!("qart: {msg}");
            }
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("qart: {e}");
            ExitCode::FAILURE
        }
    }
}
