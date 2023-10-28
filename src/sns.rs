use std::num::NonZeroU32;

use image::{GenericImage, GenericImageView, RgbaImage};

use rayon::prelude::{IntoParallelIterator, ParallelIterator};

use crate::error::{Error, Result};
use crate::random;
use crate::similarity;

pub(crate) struct SnsImpl {
    x_split: u32,
    y_split: u32,
    total_splits: usize,
}

pub(crate) struct DimensionParams {
    width: u32,
    height: u32,
    sub_width: u32,
    sub_height: u32,
}

impl SnsImpl {
    pub fn new(x_split: NonZeroU32, y_split: NonZeroU32) -> Self {
        let total_splits = (x_split.get() * y_split.get()) as usize;
        Self {
            x_split: x_split.get(),
            y_split: y_split.get(),
            total_splits,
        }
    }

    pub(crate) fn get_shuffled_indices(&self) -> Vec<usize> {
        random::get_shuffled_indices(self.total_splits)
    }

    pub(crate) fn arrange_splits(
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

    pub(crate) fn compute_dimension_params(&self, img: &RgbaImage) -> Result<DimensionParams> {
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
        Ok(DimensionParams {
            width,
            height,
            sub_width,
            sub_height,
        })
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

    pub fn decode(&self, img: &RgbaImage, params: &DimensionParams) -> Result<Vec<usize>> {
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

        let splits: Vec<_> = (0..(self.total_splits)).map(gen_view).collect();

        let x_split = self.x_split as usize;

        let nearest_pair = (0..(self.total_splits))
            .into_par_iter()
            .map(|start| {
                let mut splits_to_use: Vec<_> = splits.iter().map(Option::Some).collect();
                let mut indices: Vec<_> = Vec::with_capacity(self.total_splits);
                for y in 0..(self.y_split as usize) {
                    for x in 0..(self.x_split as usize) {
                        if x == 0 && y == 0 {
                            assert!(splits_to_use[start].is_some());
                            indices.push(start);
                            splits_to_use[start] = None;
                        } else if x == 0 {
                            let nearest_idx = splits_to_use
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| o.is_some())
                                .map(|(idx, opt)| (idx, opt.unwrap()))
                                .min_by_key(|(_, split)| {
                                    // compare with the upper split
                                    similarity::compute_border_abs_diff(
                                        &*splits[indices[(y - 1) * x_split]],
                                        *split,
                                        similarity::Direction::Down,
                                    )
                                })
                                .unwrap()
                                .0;

                            assert!(splits_to_use[nearest_idx].is_some());
                            indices.push(nearest_idx);
                            splits_to_use[nearest_idx] = None;
                        } else {
                            let nearest_idx = splits_to_use
                                .iter()
                                .enumerate()
                                .filter(|(_, o)| o.is_some())
                                .map(|(idx, opt)| (idx, opt.unwrap()))
                                .min_by_key(|(_, split)| {
                                    // compare with the left split
                                    similarity::compute_border_abs_diff(
                                        &*splits[indices[(x - 1) + y * x_split]],
                                        *split,
                                        similarity::Direction::Right,
                                    )
                                })
                                .unwrap()
                                .0;

                            assert!(splits_to_use[nearest_idx].is_some());
                            indices.push(nearest_idx);
                            splits_to_use[nearest_idx] = None;
                        }
                    }
                }

                let abs_diff = self.compute_total_abs_diff(img, &indices, params);

                (indices, abs_diff)
            })
            .min_by_key(|(_, s)| *s)
            .unwrap();

        Ok(nearest_pair.0)
    }
}
