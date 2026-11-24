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

/// Smallest two costs in each row of a border-cost matrix (diagonal excluded).
struct RowMins {
    min1: Vec<u32>,
    arg1: Vec<usize>,
    min2: Vec<u32>,
}

impl RowMins {
    fn new(matrix: &[u32], n: usize) -> Self {
        let mut mins = Self {
            min1: vec![u32::MAX; n],
            arg1: vec![0; n],
            min2: vec![u32::MAX; n],
        };
        for a in 0..n {
            for b in (0..n).filter(|&b| b != a) {
                let v = matrix[a * n + b];
                if v < mins.min1[a] {
                    mins.min2[a] = mins.min1[a];
                    mins.min1[a] = v;
                    mins.arg1[a] = b;
                } else if v < mins.min2[a] {
                    mins.min2[a] = v;
                }
            }
        }
        mins
    }

    /// Cost of `b` after `a`, relative to the best alternative after `a`.
    /// Below 1 means `b` is a clear best match; around 1 or more means the match is ambiguous.
    fn ratio(&self, matrix: &[u32], n: usize, a: usize, b: usize) -> f64 {
        let alt = if self.arg1[a] == b { self.min2[a] } else { self.min1[a] };
        if alt == u32::MAX {
            return 1.0;
        }
        (f64::from(matrix[a * n + b]) + 1.0) / (f64::from(alt) + 1.0)
    }
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
    right_mins: RowMins,
    down_mins: RowMins,
    /// For each tile `a`, all tiles sorted by `right[a][.]` (and likewise for `down`).
    right_order: Vec<u32>,
    down_order: Vec<u32>,
}

/// Returns the two best unused candidates in a sorted row, advancing monotone cursors.
/// `p1` and `p2` only move forward because tiles are never unused again.
fn front(order: &[u32], n: usize, a: usize, p1: &mut usize, p2: &mut usize, used: &[bool]) -> (Option<usize>, Option<usize>) {
    let row = &order[a * n..(a + 1) * n];
    while *p1 < n && used[row[*p1] as usize] {
        *p1 += 1;
    }
    if *p2 <= *p1 {
        *p2 = *p1 + 1;
    }
    while *p2 < n && used[row[*p2] as usize] {
        *p2 += 1;
    }
    let first = (*p1 < n).then(|| row[*p1] as usize);
    let second = (*p2 < n).then(|| row[*p2] as usize);
    (first, second)
}

/// Confidence of placing `c` after `a`: its cost relative to the best alternative
/// among unused tiles. Below 1 means `c` is a clear match.
fn confidence(matrix: &[u32], n: usize, a: usize, c: usize, first: Option<usize>, second: Option<usize>) -> f64 {
    let alt = if first == Some(c) { second } else { first };
    match alt {
        None => 1.0,
        Some(alt) => (f64::from(matrix[a * n + c]) + 1.0) / (f64::from(matrix[a * n + alt]) + 1.0),
    }
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

        let right = matrix(similarity::Direction::Right);
        let down = matrix(similarity::Direction::Down);
        let sorted_rows = |m: &[u32]| -> Vec<u32> {
            let mut order = Vec::with_capacity(n * n);
            for a in 0..n {
                let mut row: Vec<u32> = (0..n as u32).collect();
                row.sort_by_key(|&b| m[a * n + b as usize]);
                order.extend_from_slice(&row);
            }
            order
        };
        Self {
            n,
            right_mins: RowMins::new(&right, n),
            down_mins: RowMins::new(&down, n),
            right_order: sorted_rows(&right),
            down_order: sorted_rows(&down),
            right,
            down,
        }
    }

    /// Row-major greedy growth from `seed`: each cell takes the unused tile with the
    /// lowest raw border cost to its left or upper neighbor.
    fn grow_rows(&self, seed: usize, x_split: usize) -> Vec<usize> {
        let n = self.n;
        let mut used = vec![false; n];
        let mut indices: Vec<usize> = Vec::with_capacity(n);
        used[seed] = true;
        indices.push(seed);
        for pos in 1..n {
            let next = if pos % x_split == 0 {
                let upper = indices[pos - x_split];
                (0..n)
                    .filter(|&c| !used[c])
                    .min_by_key(|&c| self.down[upper * n + c])
                    .unwrap()
            } else {
                let left = indices[pos - 1];
                (0..n)
                    .filter(|&c| !used[c])
                    .min_by_key(|&c| self.right[left * n + c])
                    .unwrap()
            };
            used[next] = true;
            indices.push(next);
        }
        indices
    }

    /// Grows the arrangement from `seed` on a torus of `x_split` x `y_split` cells.
    /// At each step it places the most confident (cell, tile) pair among the empty
    /// cells that have a placed left or upper neighbor. Uncertain placements are
    /// deferred until their neighborhood is more constrained.
    fn grow(&self, seed: usize, x_split: usize, y_split: usize) -> Vec<usize> {
        let n = self.n;
        let mut used = vec![false; n];
        let mut cell = vec![usize::MAX; n];
        let (mut r1, mut r2) = (vec![0_usize; n], vec![0_usize; n]);
        let (mut d1, mut d2) = (vec![0_usize; n], vec![0_usize; n]);

        cell[0] = seed;
        used[seed] = true;
        for _ in 1..n {
            // (score, raw cost, cell, tile): lower is better, raw cost breaks ties.
            let mut best: Option<(f64, u64, usize, usize)> = None;
            for p in 0..n {
                if cell[p] != usize::MAX {
                    continue;
                }
                let (r, col) = (p / x_split, p % x_split);
                let left = r * x_split + (col + x_split - 1) % x_split;
                let up = ((r + y_split - 1) % y_split) * x_split + col;
                let la = (cell[left] != usize::MAX).then(|| cell[left]);
                let ua = (cell[up] != usize::MAX).then(|| cell[up]);
                if la.is_none() && ua.is_none() {
                    continue;
                }

                let mut candidates = [None, None];
                if let Some(a) = la {
                    candidates[0] = front(&self.right_order, n, a, &mut r1[a], &mut r2[a], &used).0;
                }
                if let Some(u) = ua {
                    candidates[1] = front(&self.down_order, n, u, &mut d1[u], &mut d2[u], &used).0;
                }

                for c in candidates.into_iter().flatten() {
                    let mut score = f64::INFINITY;
                    let mut raw = 0_u64;
                    if let Some(a) = la {
                        let (f, s) = front(&self.right_order, n, a, &mut r1[a], &mut r2[a], &used);
                        score = score.min(confidence(&self.right, n, a, c, f, s));
                        raw += u64::from(self.right[a * n + c]);
                    }
                    if let Some(u) = ua {
                        let (f, s) = front(&self.down_order, n, u, &mut d1[u], &mut d2[u], &used);
                        score = score.min(confidence(&self.down, n, u, c, f, s));
                        raw += u64::from(self.down[u * n + c]);
                    }
                    let better = match best {
                        None => true,
                        Some((bs, br, _, _)) => score < bs || (score == bs && raw < br),
                    };
                    if better {
                        best = Some((score, raw, p, c));
                    }
                }
            }

            let (_, _, p, c) = best.expect("an empty cell next to a placed tile always exists");
            cell[p] = c;
            used[c] = true;
        }
        cell
    }

    /// Mean confidence (see `RowMins::ratio`) over the interior seams of an arrangement.
    /// Lower means the arrangement is made of clearer matches.
    fn mean_confidence(&self, indices: &[usize], x_split: usize) -> f64 {
        let n = self.n;
        let rows = indices.len() / x_split;
        let mut sum = 0.0;
        let mut count = 0_usize;
        for (pos, &a) in indices.iter().enumerate() {
            let (x, y) = (pos % x_split, pos / x_split);
            if x + 1 < x_split {
                sum += self.right_mins.ratio(&self.right, n, a, indices[pos + 1]);
                count += 1;
            }
            if y + 1 < rows {
                sum += self.down_mins.ratio(&self.down, n, a, indices[pos + x_split]);
                count += 1;
            }
        }
        if count == 0 {
            0.0
        } else {
            sum / count as f64
        }
    }

    /// Picks the cyclic shift (rows and columns) of a grid whose outer seams are the
    /// least confident matches. The outer seams are the pairs that would wrap around
    /// (last row to first row, last column to first column). Interior-only costs
    /// prefer to cut at the most expensive seam, which is the failure this avoids.
    fn orient(&self, grid: &[usize], x_split: usize, y_split: usize) -> Vec<usize> {
        let n = self.n;
        let at = |ky: usize, kx: usize, r: usize, c: usize| {
            grid[((r + ky) % y_split) * x_split + (c + kx) % x_split]
        };

        let mut best = (f64::NEG_INFINITY, 0, 0);
        for ky in 0..y_split {
            for kx in 0..x_split {
                let mut score = 0.0;
                if y_split >= 2 {
                    for c in 0..x_split {
                        let (a, b) = (at(ky, kx, y_split - 1, c), at(ky, kx, 0, c));
                        score += self.down_mins.ratio(&self.down, n, a, b);
                    }
                }
                if x_split >= 2 {
                    for r in 0..y_split {
                        let (a, b) = (at(ky, kx, r, x_split - 1), at(ky, kx, r, 0));
                        score += self.right_mins.ratio(&self.right, n, a, b);
                    }
                }
                if score > best.0 {
                    best = (score, ky, kx);
                }
            }
        }

        let (_, ky, kx) = best;
        (0..n)
            .map(|pos| at(ky, kx, pos / x_split, pos % x_split))
            .collect()
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

        // For each seed, build one arrangement with row-major greedy growth and one with
        // confidence-first growth. Orient each, then keep the arrangement with the lowest
        // mean seam confidence across all seeds and both methods.
        let y_split = self.y_split as usize;
        let best = (0..2 * n)
            .into_par_iter()
            .map(|k| {
                let start = k / 2;
                let grid = if k % 2 == 0 {
                    costs.grow_rows(start, x_split)
                } else {
                    costs.grow(start, x_split, y_split)
                };
                let indices = costs.orient(&grid, x_split, y_split);
                let conf = costs.mean_confidence(&indices, x_split);
                (indices, conf)
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();

        Ok(best.0)
    }
}
