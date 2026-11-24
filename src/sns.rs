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

/// Border costs between every ordered pair of tiles, computed once per decode.
///
/// `right[a * n + b]` is the cost of placing tile `b` directly right of tile `a`,
/// and `down[a * n + b]` is the cost of placing `b` directly below `a`.
/// The matrices are not symmetric.
struct BorderCosts {
    n: usize,
    right: Vec<u32>,
    down: Vec<u32>,
}

impl BorderCosts {
    fn new(img: &RgbaImage, sns: &SnsImpl, params: &DimensionParams) -> Self {
        let n = sns.total_splits;
        let x_split = sns.x_split as usize;
        let tiles: Vec<_> = (0..n)
            .map(|idx| {
                let i = idx % x_split;
                let j = idx / x_split;
                img.view(
                    (i as u32) * params.sub_width,
                    (j as u32) * params.sub_height,
                    params.sub_width,
                    params.sub_height,
                )
            })
            .collect();

        let matrix = |dir: similarity::Direction| -> Vec<u32> {
            (0..n * n)
                .into_par_iter()
                .map(|k| {
                    let (a, b) = (k / n, k % n);
                    if a == b {
                        return 0;
                    }
                    // Each cost is a sum of one border (at most 1020 * tile side),
                    // so it fits in u32 for any image this tool can handle.
                    u32::try_from(similarity::compute_border_abs_diff(&*tiles[a], &*tiles[b], dir))
                        .expect("border cost does not fit in u32")
                })
                .collect()
        };

        Self {
            n,
            right: matrix(similarity::Direction::Right),
            down: matrix(similarity::Direction::Down),
        }
    }

    /// Sums the border costs of all adjacent pairs in a full arrangement.
    /// `indices[pos]` is the tile id placed at row-major position `pos`.
    fn total(&self, indices: &[usize], x_split: usize) -> u64 {
        let n = self.n;
        let mut sum = 0_u64;
        for (pos, &a) in indices.iter().enumerate() {
            let x = pos % x_split;
            let y = pos / x_split;
            if x + 1 < x_split {
                sum += u64::from(self.right[a * n + indices[pos + 1]]);
            }
            if (y + 1) * x_split < indices.len() {
                sum += u64::from(self.down[a * n + indices[pos + x_split]]);
            }
        }
        sum
    }
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

    pub fn decode(&self, img: &RgbaImage, params: &DimensionParams) -> Result<Vec<usize>> {
        let costs = BorderCosts::new(img, self, params);
        let n = self.total_splits;
        let x_split = self.x_split as usize;

        // Greedy placement from each possible start tile. Candidates are scanned
        // in ascending index order and `min_by_key` keeps the first minimum, so the
        // result is identical to the earlier per-step computation.
        let nearest_pair = (0..n)
            .into_par_iter()
            .map(|start| {
                let mut used = vec![false; n];
                let mut indices: Vec<usize> = Vec::with_capacity(n);
                used[start] = true;
                indices.push(start);

                for pos in 1..n {
                    let x = pos % x_split;
                    let next = if x == 0 {
                        // First column: compare with the tile above.
                        let upper = indices[pos - x_split];
                        (0..n)
                            .filter(|&c| !used[c])
                            .min_by_key(|&c| costs.down[upper * n + c])
                            .unwrap()
                    } else {
                        // Other columns: compare with the tile on the left.
                        let left = indices[pos - 1];
                        (0..n)
                            .filter(|&c| !used[c])
                            .min_by_key(|&c| costs.right[left * n + c])
                            .unwrap()
                    };
                    used[next] = true;
                    indices.push(next);
                }

                let abs_diff = costs.total(&indices, x_split);
                (indices, abs_diff)
            })
            .min_by_key(|(_, s)| *s)
            .unwrap();

        Ok(nearest_pair.0)
    }
}
