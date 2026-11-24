use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

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

#[derive(thiserror::Error, Debug)]
enum CliError {
    #[error(transparent)]
    Lib(#[from] slice_n_shuffle::Error),
    #[error("Invalid crop specification \"{0}\": expected LEFT,RIGHT,TOP,BOTTOM as four non-negative integers")]
    InvalidCrop(String),
    #[error("The crop (left: {left}, right: {right}, top: {top}, bottom: {bottom}) leaves no pixels in the image ({width}, {height})")]
    CropOutOfRange {
        width: u32,
        height: u32,
        left: u32,
        right: u32,
        top: u32,
        bottom: u32,
    },
    #[error("Failed to get the file name from the path \"{}\"", .0.display())]
    NoFileName(PathBuf),
}

struct CropArea {
    left: u32,
    right: u32,
    top: u32,
    bottom: u32,
}

impl TryFrom<String> for CropArea {
    type Error = CliError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let invalid = || CliError::InvalidCrop(value.clone());
        let v = value
            .split(',')
            .map(|s| s.trim().parse::<u32>())
            .collect::<Result<Vec<u32>, _>>()
            .map_err(|_| invalid())?;
        match v.as_slice() {
            &[left, right, top, bottom] => Ok(Self {
                left,
                right,
                top,
                bottom,
            }),
            _ => Err(invalid()),
        }
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
) -> Result<OutputPath, CliError> {
    let output_name = {
        let mut name = input
            .as_ref()
            .file_stem()
            .ok_or_else(|| CliError::NoFileName(input.as_ref().to_owned()))?
            .to_owned();
        name.push(suffix);
        name.push(".png");
        name
    };

    Ok(match output {
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
    })
}

fn load_image(from: &Path, crop: Option<CropArea>) -> Result<RgbaImage, CliError> {
    let mut img = image::ImageReader::open(from)
        .map_err(slice_n_shuffle::Error::Open)?
        .decode()
        .map_err(slice_n_shuffle::Error::Decode)?
        .into_rgba8();

    Ok(if let Some(crop) = crop {
        let (w, h) = img.dimensions();
        let width = w
            .checked_sub(crop.left)
            .and_then(|v| v.checked_sub(crop.right));
        let height = h
            .checked_sub(crop.top)
            .and_then(|v| v.checked_sub(crop.bottom));
        match (width, height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => {
                image::imageops::crop(&mut img, crop.left, crop.top, width, height).to_image()
            }
            _ => {
                return Err(CliError::CropOutOfRange {
                    width: w,
                    height: h,
                    left: crop.left,
                    right: crop.right,
                    top: crop.top,
                    bottom: crop.bottom,
                })
            }
        }
    } else {
        img
    })
}

fn encode(args: Args) -> Result<(), CliError> {
    let input = args.input;

    let output = determine_output_path(&input, args.output, "_encoded")?;

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

fn decode(args: Args) -> Result<(), CliError> {
    let input = args.input;

    let output = determine_output_path(&input, args.output, "_decoded")?;

    let crop: Option<CropArea> = if let Some(s) = args.crop {
        Some(s.try_into()?)
    } else {
        None
    };

    eprintln!("From: \"{}\"", input.display());
    eprintln!("To: \"{output}\"");

    let img = load_image(&input, crop)?;

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
