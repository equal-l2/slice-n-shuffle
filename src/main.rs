use clap::Parser;
use std::{
    num::NonZeroU32,
    path::{Path, PathBuf},
};

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
            OutputPath::Implicit(path) => path,
            OutputPath::Explicit(path) => path,
        }
    }
}

impl std::fmt::Display for OutputPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OutputPath::Implicit(path) => write!(f, "{}", path.display()),
            OutputPath::Explicit(path) => write!(f, "{}", path.display()),
        }
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

fn encode(args: Args) {
    let input = args.input;

    let output = determine_output_path(&input, args.output, "_encoded");

    eprintln!("From: \"{}\"", input.display());
    eprintln!("To: \"{}\"", output);

    if let Err(e) = slice_n_shuffle::encode_with_path(&input, &output, args.x_split, args.y_split) {
        eprintln!("Error: {e}")
    }
}

fn decode(args: Args) {
    let input = args.input;

    let output = determine_output_path(&input, args.output, "_decoded");

    eprintln!("From: \"{}\"", input.display());
    eprintln!("To: \"{}\"", output);

    if let Err(e) = slice_n_shuffle::decode_with_path(&input, &output, args.x_split, args.y_split) {
        eprintln!("Error: {e}")
    }
}

fn main() {
    let app = App::parse();

    match app.command {
        Commands::Encode(args) => encode(args),
        Commands::Decode(args) => decode(args),
    }
}
