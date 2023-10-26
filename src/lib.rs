use std::num::NonZeroU32;
use std::path::Path;

use image::{GenericImage, GenericImageView, RgbaImage};

use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use rand_xoshiro::SplitMix64 as TheRng;

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
        x_split: NonZeroU32,
        y_split: NonZeroU32,
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

thread_local! {
    static RNG_KEY: TheRng = init_rng().unwrap();
}

fn get_indices_shuffled<R: Rng>(rng: &mut R, size: usize) -> Result<Vec<usize>> {
    let mut seq = (0..size).collect::<Vec<_>>();
    seq.shuffle(rng);
    Ok(seq)
}

fn init_rng() -> Result<TheRng> {
    let mut buf = [0; 8];
    getrandom::getrandom(&mut buf).map_err(Error::GetEntropy)?;
    Ok(TheRng::from_seed(buf))
}

pub struct SliceNShuffle {
    x_split: NonZeroU32,
    y_split: NonZeroU32,
}

impl SliceNShuffle {
    pub fn new(x_split: NonZeroU32, y_split: NonZeroU32) -> Self {
        Self { x_split, y_split }
    }

    pub fn read_path_and_save<P1, P2>(&self, from: P1, to: P2) -> Result<()>
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let img = image::io::Reader::open(from)
            .map_err(Error::Open)?
            .decode()
            .map_err(Error::Decode)?
            .into_rgba8();
        let img_out = self.split_n_shuffle(&img)?;
        img_out.save(to).map_err(Error::Save)
    }

    pub fn split_n_shuffle(&self, img: &RgbaImage) -> Result<RgbaImage> {
        let (width, height) = img.dimensions();
        if width % self.x_split != 0 || height % self.y_split != 0 {
            return Err(Error::DimensionMismatch {
                width,
                height,
                x_split: self.x_split,
                y_split: self.y_split,
            });
        }

        let sub_width = width / self.x_split;
        let sub_height = height / self.y_split;
        let total_split = (self.x_split.get() * self.y_split.get()) as usize;

        let mut rng = RNG_KEY.with(|t| t.clone());
        let indices = get_indices_shuffled(&mut rng, total_split)?;

        let mut img_out = RgbaImage::new(width, height);

        for (idx_from, idx_to) in indices.into_iter().enumerate() {
            let x_split = self.x_split.get() as usize;
            let i_from = idx_from % x_split;
            let j_from = idx_from / x_split;
            let i_to = idx_to % x_split;
            let j_to = idx_to / x_split;

            let from_split = img.view(
                (i_from as u32) * sub_width,
                (j_from as u32) * sub_height,
                sub_width,
                sub_height,
            );
            let mut to_split = img_out.sub_image(
                (i_to as u32) * sub_width,
                (j_to as u32) * sub_height,
                sub_width,
                sub_height,
            );

            to_split
                .copy_from(&*from_split, 0, 0)
                .map_err(Error::Convert)?;
        }

        Ok(img_out)
    }
}
