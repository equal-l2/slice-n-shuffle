#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Failed to open image: {0}")]
    Open(std::io::Error),
    #[error("Failed to get image dimensions: {0}")]
    ReadDimensions(image::ImageError),
    #[error("Dimensions ({width}, {height}) are not divisible by ({x_split}, {y_split})")]
    DimensionMismatch {
        width: u32,
        height: u32,
        x_split: u32,
        y_split: u32,
    },
    #[error("Failed to initialize RNG due to failure of entropy retrieval: {0}")]
    GetEntropy(getrandom::Error),
    #[error("Failed to decode the image: {0}")]
    Decode(image::ImageError),
    #[error("Failed to convert the image: {0}")]
    Convert(image::ImageError),
    #[error("Failed to save the image: {0}")]
    Save(image::ImageError),
}

pub type Result<T> = std::result::Result<T, Error>;
