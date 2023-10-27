use std::num::NonZeroU32;
use std::path::Path;

use image::{GenericImage, GenericImageView, RgbaImage};

use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};

use rand_xoshiro::SplitMix64 as TheRng;

mod similarity;

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
    x_split: u32,
    y_split: u32,
}

struct DimensionParams {
    width: u32,
    height: u32,
    sub_width: u32,
    sub_height: u32,
    total_splits: usize,
}

impl SliceNShuffle {
    pub fn new(x_split: NonZeroU32, y_split: NonZeroU32) -> Self {
        Self {
            x_split: x_split.get(),
            y_split: y_split.get(),
        }
    }

    fn arrange_splits(
        &self,
        img: &RgbaImage,
        params: &DimensionParams,
        indices_from: &[usize],
    ) -> Result<RgbaImage> {
        let mut img_out = RgbaImage::new(params.width, params.height);
        for (idx_to, idx_from) in indices_from.iter().enumerate() {
            let x_split = self.x_split as usize;
            let i_from = idx_from % x_split;
            let j_from = idx_from / x_split;
            let i_to = idx_to % x_split;
            let j_to = idx_to / x_split;

            let from_split = img.view(
                (i_from as u32) * params.sub_width,
                (j_from as u32) * params.sub_height,
                params.sub_width,
                params.sub_height,
            );
            let mut to_split = img_out.sub_image(
                (i_to as u32) * params.sub_width,
                (j_to as u32) * params.sub_height,
                params.sub_width,
                params.sub_height,
            );

            to_split
                .copy_from(&*from_split, 0, 0)
                .map_err(Error::Convert)?;
        }

        Ok(img_out)
    }
    pub fn encode_from_path_and_save<P1, P2>(&self, from: P1, to: P2) -> Result<()>
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let img = image::io::Reader::open(from)
            .map_err(Error::Open)?
            .decode()
            .map_err(Error::Decode)?
            .into_rgba8();
        let img_out = self.encode_image(&img)?;
        img_out.save(to).map_err(Error::Save)
    }

    fn compute_dimension_params(&self, img: &RgbaImage) -> Result<DimensionParams> {
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
        let total_splits = (self.x_split * self.y_split) as usize;
        Ok(DimensionParams {
            width,
            height,
            sub_width,
            sub_height,
            total_splits,
        })
    }

    pub fn encode_image(&self, img: &RgbaImage) -> Result<RgbaImage> {
        let params = self.compute_dimension_params(img)?;

        let mut rng = RNG_KEY.with(|t| t.clone());
        let indices = get_indices_shuffled(&mut rng, params.total_splits)?;

        self.arrange_splits(img, &params, &indices)
    }

    pub fn decode_from_path_and_save<P1, P2>(&self, from: P1, to: P2) -> Result<()>
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let img = image::io::Reader::open(from)
            .map_err(Error::Open)?
            .decode()
            .map_err(Error::Decode)?
            .into_rgba8();
        let img_out = self.decode_image(&img)?;
        img_out.save(to).map_err(Error::Save)
    }

    fn compute_total_abs_diff(
        &self,
        img: &RgbaImage,
        indices: &[usize],
        params: &DimensionParams,
    ) -> u64 {
        let gen_view = |idx| {
            let x_split = self.x_split as usize;
            let i = idx % x_split;
            let j = idx / x_split;
            img.view(
                (i as u32) * params.sub_width,
                (j as u32) * params.sub_height,
                params.sub_width,
                params.sub_height,
            )
        };

        let splits: Vec<_> = indices.iter().copied().map(gen_view).collect();

        let mut similarity = 0;
        let x_split = self.x_split as usize;
        let y_split = self.y_split as usize;

        // vertical similarity
        for x in 0..x_split {
            for y in 0..(y_split - 1) {
                let upper = splits[x + y * x_split];
                let lower = splits[x + (y + 1) * x_split];

                similarity += similarity::compute_border_abs_diff(
                    &*upper,
                    &*lower,
                    similarity::Direction::Down,
                );
            }
        }

        // horizontal similarity
        for y in 0..y_split {
            for x in 0..(x_split - 1) {
                let left = splits[x + y * x_split];
                let right = splits[x + y * x_split + 1];

                similarity += similarity::compute_border_abs_diff(
                    &*left,
                    &*right,
                    similarity::Direction::Right,
                );
            }
        }

        similarity
    }

    pub fn decode_image(&self, img: &RgbaImage) -> Result<RgbaImage> {
        let params = self.compute_dimension_params(img)?;

        //let mut rng = RNG_KEY.with(|t| t.clone());

        let gen_view = |idx| {
            let x_split = self.x_split as usize;
            let i = idx % x_split;
            let j = idx / x_split;
            img.view(
                (i as u32) * params.sub_width,
                (j as u32) * params.sub_height,
                params.sub_width,
                params.sub_height,
            )
        };

        let splits: Vec<_> = (0..(params.total_splits)).map(gen_view).collect();

        // for i in 0..(params.total_splits) {
        //     for j in 0..(params.total_splits) {
        //         let res = similarity::compute_border_abs_diff(
        //             &*splits[i],
        //             &*splits[j],
        //             similarity::Direction::Right,
        //         );
        //         println!("{} vs {} (Right): {}", i, j, res);

        //         let res = similarity::compute_border_abs_diff(
        //             &*splits[i],
        //             &*splits[j],
        //             similarity::Direction::Down,
        //         );
        //         println!("{} vs {} (Down): {}", i, j, res);
        //     }
        // }

        let x_split = self.x_split as usize;

        let nearest_pair = (0..(params.total_splits))
            .map(|start| {
                let mut splits_to_use: Vec<_> = splits.iter().map(Option::Some).collect();
                let mut indices: Vec<_> = Vec::with_capacity(params.total_splits);
                // println!("start: {}", start);
                for y in 0..(self.y_split as usize) {
                    for x in 0..(self.x_split as usize) {
                        if x == 0 && y == 0 {
                            assert!(splits_to_use[start].is_some());
                            indices.push(start);
                            splits_to_use[start] = None;
                        } else if x == 0 {
                            // compare with the upper split
                            let target_split = splits[(y - 1) * x_split];

                            let nearest_idx = splits_to_use
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| o.is_some())
                                .map(|(idx, opt)| (idx, opt.unwrap()))
                                .min_by_key(|(i, split)| {
                                    let res = similarity::compute_border_abs_diff(
                                        &*target_split,
                                        *split,
                                        similarity::Direction::Down,
                                    );
                                    // println!(
                                    //     "{} vs {} (Down): {}",
                                    //     indices[(y - 1) * x_split],
                                    //     i,
                                    //     res
                                    // );
                                    res
                                })
                                .unwrap()
                                .0;

                            assert!(splits_to_use[nearest_idx].is_some());
                            indices.push(nearest_idx);
                            splits_to_use[nearest_idx] = None;
                        } else {
                            // compare with the left split
                            let target_split = splits[(x - 1) + y * x_split];

                            let nearest_idx = splits_to_use
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| o.is_some())
                                .map(|(idx, opt)| (idx, opt.unwrap()))
                                .min_by_key(|(i, split)| {
                                    let res = similarity::compute_border_abs_diff(
                                        &*target_split,
                                        *split,
                                        similarity::Direction::Right,
                                    );
                                    // println!(
                                    //     "{} vs {} (Right): {}",
                                    //     indices[(x - 1) + y * x_split],
                                    //     i,
                                    //     res
                                    // );
                                    res
                                })
                                .unwrap()
                                .0;

                            assert!(splits_to_use[nearest_idx].is_some());
                            indices.push(nearest_idx);
                            splits_to_use[nearest_idx] = None;
                        }
                    }
                }

                let abs_diff = self.compute_total_abs_diff(img, &indices, &params);

                // println!("{:?} ({})", indices, abs_diff);
                // self.arrange_splits(img, &params, &indices)
                //     .unwrap()
                //     .save(format!("dbg/{start}.png"))
                //     .unwrap();

                (indices, abs_diff)
            })
            .min_by_key(|(_, s)| *s)
            .unwrap();

        self.arrange_splits(img, &params, &nearest_pair.0)
    }
}
