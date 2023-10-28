use std::{num::NonZeroU32, path::Path};

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
