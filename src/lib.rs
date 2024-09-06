use std::num::NonZeroU32;

mod error;
mod random;
mod similarity;
mod sns;

use error::Result;

use image::RgbaImage;
use sns::SnsImpl;

pub use error::Error;

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

    fn encode_to_png(img: image::RgbaImage) -> Result<Vec<u8>, super::Error> {
        let mut buf = vec![];
        let encoder = image::codecs::png::PngEncoder::new(&mut buf);
        img.write_with_encoder(encoder)
            .map_err(super::Error::Encode)?;
        Ok(buf)
    }

    #[wasm_bindgen]
    pub fn encode_image_buffer(buf: &[u8], x_split: u32, y_split: u32) -> Result<Vec<u8>, JsValue> {
        let (x_split, y_split) = convert_to_nonzero_u32(x_split, y_split)?;
        let img = decode_into_image(buf).map_err(Into::<JsValue>::into)?;
        let img_out = super::encode_image(&img, x_split, y_split).map_err(Into::<JsValue>::into)?;
        encode_to_png(img_out).map_err(Into::<JsValue>::into)
    }

    #[wasm_bindgen]
    pub fn decode_image_buffer(buf: &[u8], x_split: u32, y_split: u32) -> Result<Vec<u8>, JsValue> {
        let (x_split, y_split) = convert_to_nonzero_u32(x_split, y_split)?;
        let img = decode_into_image(buf).map_err(Into::<JsValue>::into)?;
        let img_out = super::decode_image(&img, x_split, y_split).map_err(Into::<JsValue>::into)?;
        encode_to_png(img_out).map_err(Into::<JsValue>::into)
    }
}

#[cfg(feature = "js")]
pub use wasm_fn::*;
