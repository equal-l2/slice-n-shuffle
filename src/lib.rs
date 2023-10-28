use std::num::NonZeroU32;
use std::path::Path;

mod error;
mod random;
mod similarity;
mod sns;

use error::{Error, Result};

use image::RgbaImage;
use sns::SnsImpl;

pub fn encode_image(
    img: &RgbaImage,
    x_split: NonZeroU32,
    y_split: NonZeroU32,
) -> Result<RgbaImage> {
    let sns = SnsImpl::new(x_split, y_split);
    let params = sns.compute_dimension_params(img)?;
    let indices = sns.get_shuffled_indices();
    sns.arrange_splits(img, &params, &indices)
}

pub fn decode_image(
    img: &RgbaImage,
    x_split: NonZeroU32,
    y_split: NonZeroU32,
) -> Result<RgbaImage> {
    let sns = SnsImpl::new(x_split, y_split);
    let params = sns.compute_dimension_params(img)?;
    let indices = sns.decode(img, &params)?;
    sns.arrange_splits(img, &params, &indices)
}

fn load_and_save_with<F>(from: &Path, to: &Path, f: F) -> Result<()>
where
    F: FnOnce(&RgbaImage) -> Result<RgbaImage>,
{
    let img = image::io::Reader::open(from)
        .map_err(Error::Open)?
        .decode()
        .map_err(Error::Decode)?
        .into_rgba8();
    let img_out = f(&img)?;
    img_out.save(to).map_err(Error::Save)
}

pub fn encode_with_path(
    from: &Path,
    to: &Path,
    x_split: NonZeroU32,
    y_split: NonZeroU32,
) -> Result<()> {
    load_and_save_with(from, to, |img| encode_image(img, x_split, y_split))
}

pub fn decode_with_path(
    from: &Path,
    to: &Path,
    x_split: NonZeroU32,
    y_split: NonZeroU32,
) -> Result<()> {
    load_and_save_with(from, to, |img| decode_image(img, x_split, y_split))
}

#[cfg(feature = "js")]
mod wasm_fn {
    use std::num::NonZeroU32;

    use wasm_bindgen::prelude::*;

    fn convert_to_nonzero_u32(
        x_split: u32,
        y_split: u32,
    ) -> Result<(NonZeroU32, NonZeroU32), JsValue> {
        let x_split = NonZeroU32::new(x_split).ok_or("x_split must be a non-zero number")?;
        let y_split = NonZeroU32::new(y_split).ok_or("x_split must be a non-zero number")?;
        Ok((x_split, y_split))
    }

    fn decode_into_image(buf: &[u8]) -> Result<image::RgbaImage, super::Error> {
        image::load_from_memory(buf)
            .map_err(super::Error::Decode)
            .map(|i| i.to_rgba8())
    }

    #[wasm_bindgen]
    pub fn encode_image_buffer(buf: &[u8], x_split: u32, y_split: u32) -> Result<Vec<u8>, JsValue> {
        let (x_split, y_split) = convert_to_nonzero_u32(x_split, y_split)?;
        let img = decode_into_image(buf).map_err(Into::<JsValue>::into)?;
        let img_out = super::encode_image(&img, x_split, y_split).map_err(Into::<JsValue>::into)?;
        Ok(img_out.into_raw())
    }

    #[wasm_bindgen]
    pub fn decode_image_buffer(buf: &[u8], x_split: u32, y_split: u32) -> Result<Vec<u8>, JsValue> {
        let (x_split, y_split) = convert_to_nonzero_u32(x_split, y_split)?;
        let img = decode_into_image(buf).map_err(Into::<JsValue>::into)?;
        let img_out = super::decode_image(&img, x_split, y_split).map_err(Into::<JsValue>::into)?;
        Ok(img_out.into_raw())
    }
}

#[cfg(feature = "js")]
pub use wasm_fn::*;
