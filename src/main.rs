use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

use anyhow::Result;
use clap::Parser;
use image::RgbaImage;

#[derive(Parser)]
struct App {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    Encode(Args),
    Decode(Args),
}

#[derive(clap::Args)]
struct Args {
    input: PathBuf,
    x_split: NonZeroU32,
    y_split: NonZeroU32,
    #[clap(short, long)]
    output: Option<PathBuf>,
    #[clap(long)]
    crop: Option<String>,
}

struct CropArea {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl TryFrom<String> for CropArea {
    type Error = std::num::ParseIntError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let v: Vec<u32> = value
            .split(',')
            .map(str::trim)
            .map(str::parse)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            left: v[0],
            right: v[1],
            top: v[2],
            bottom: v[3],
        })
    }
}

#[derive(Debug)]
enum OutputPath {
    Implicit(PathBuf),
    Explicit(PathBuf),
}

impl std::ops::Deref for OutputPath {
    type Target = Path;
    fn deref(&self) -> &Path {
        match self {
            Self::Implicit(path) | Self::Explicit(path) => path,
        }
    }
}

impl AsRef<Path> for OutputPath {
    fn as_ref(&self) -> &Path {
        self
    }
}

impl std::fmt::Display for OutputPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display())
    }
}

fn determine_output_path<P: AsRef<Path>>(
    input: &P,
    output: Option<PathBuf>,
    suffix: &str,
) -> OutputPath {
    let output_name = {
        let mut name = input.as_ref().file_stem().unwrap().to_owned();
        name.push(suffix);
        name.push(".png");
        name
    };

    match output {
        None => {
            let mut output_path = input.as_ref().to_owned();
            output_path.set_file_name(output_name);
            OutputPath::Implicit(output_path)
        }
        Some(inner) => {
            if inner.is_dir() {
                let mut output_path = inner;
                output_path.push(output_name);
                OutputPath::Implicit(output_path)
            } else {
                OutputPath::Explicit(inner)
            }
        }
    }
}

fn load_image(from: &Path, crop: Option<CropArea>) -> Result<RgbaImage> {
    let mut img = image::ImageReader::open(from)
        .map_err(slice_n_shuffle::Error::Open)?
        .decode()
        .map_err(slice_n_shuffle::Error::Decode)?
        .into_rgba8();

    Ok(if let Some(crop) = crop {
        let x = crop.left;
        let y = crop.top;
        let width = img.width() - crop.right;
        let height = img.height() - crop.bottom;
        image::imageops::crop(&mut img, x, y, width, height).to_image()
    } else {
        img
    })
}

fn encode(args: Args) -> Result<()> {
    let input = args.input;

    let output = determine_output_path(&input, args.output, "_encoded");

    let crop: Option<CropArea> = if let Some(s) = args.crop {
        Some(s.try_into()?)
    } else {
        None
    };

    eprintln!("From: \"{}\"", input.display());
    eprintln!("To: \"{output}\"");

    slice_n_shuffle::encode_image(&load_image(&input, crop)?, args.x_split, args.y_split)?
        .save(output)
        .map_err(slice_n_shuffle::Error::Save)
        .map_err(Into::into)
}

fn decode(args: Args) -> Result<()> {
    let input = args.input;

    let output = determine_output_path(&input, args.output, "_decoded");

    let crop: Option<CropArea> = if let Some(s) = args.crop {
        Some(s.try_into()?)
    } else {
        None
    };

    eprintln!("From: \"{}\"", input.display());
    eprintln!("To: \"{output}\"");

    let img = load_image(&input, crop)?;
    img.save("out.png").unwrap();

    slice_n_shuffle::decode_image(&img, args.x_split, args.y_split)?
        .save(output)
        .map_err(slice_n_shuffle::Error::Save)
        .map_err(Into::into)
}

fn main() {
    let app = App::parse();

    let res = match app.command {
        Commands::Encode(args) => encode(args),
        Commands::Decode(args) => decode(args),
    };

    if let Err(e) = res {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
