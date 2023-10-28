#[derive(thiserror::Error, Debug)]
#[cfg_attr(feature = "js", derive(serde_with::SerializeDisplay))]
pub enum Error {
    #[error("Failed to open the image: {0}")]
    Open(std::io::Error),
    #[error("The dimensions of the image ({width}, {height}) are not divisible by ({x_split}, {y_split})")]
    DimensionMismatch {
        width: u32,
        height: u32,
        x_split: u32,
        y_split: u32,
    },
    #[error("Failed to open the image in decoding: {0}")]
    Decode(image::ImageError),
    #[error("Failed to convert the image: {0}")]
    Convert(image::ImageError),
    #[error("Failed to save the image: {0}")]
    Save(image::ImageError),
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(feature = "js")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "js")]
impl From<Error> for JsValue {
    fn from(e: Error) -> Self {
        serde_wasm_bindgen::to_value(&e).unwrap()
    }
}
