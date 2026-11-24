//! Evaluation harness for `decode_image`.
//!
//! Usage:
//!   cargo run --release --example eval -- [--splits 4,8,16,32] [--trials 3] [IMAGE ...]
//!
//! The harness shuffles tiles itself with a fixed seed, so the true layout is
//! known and every run with the same build and arguments is identical. Decoding
//! has no randomness, so the printed hashes can be diffed between builds to check
//! whether the decoded output changed.

use std::collections::HashMap;
use std::num::NonZeroU32;
use std::time::Instant;

use image::RgbaImage;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand_xoshiro::SplitMix64;

const SYNTHETIC_SIZE: u32 = 256;

struct Case {
    name: String,
    img: RgbaImage,
}

/// Deterministic hash-based noise, so the generated images do not depend on any RNG API.
fn mix(mut x: u64) -> u64 {
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

fn synthetic_cases() -> Vec<Case> {
    let n = SYNTHETIC_SIZE;
    let pixel_seed = |x: u32, y: u32, c: u32| u64::from(x) * 1_000_003 + u64::from(y) * 1009 + u64::from(c) * 7;

    // Gradient with +-6 noise: every tile is byte-unique, but neighbors are similar.
    let mut grad = RgbaImage::new(n, n);
    for x in 0..n {
        for y in 0..n {
            let noise = |c: u32| (mix(pixel_seed(x, y, c)) % 13) as i32 - 6;
            let clamp = |v: i32| v.clamp(0, 255) as u8;
            grad.put_pixel(
                x,
                y,
                image::Rgba([
                    clamp(x as i32 * 2 + y as i32 / 3 + noise(0)),
                    clamp(y as i32 * 2 + noise(1)),
                    clamp((x + y) as i32 + noise(2)),
                    255,
                ]),
            );
        }
    }

    // Independent random bytes: no continuity across tile borders, so the
    // neighbor structure cannot be recovered. This is a lower bound, not a target.
    let mut noise_img = RgbaImage::new(n, n);
    for x in 0..n {
        for y in 0..n {
            let b = |c: u32| mix(pixel_seed(x, y, c) ^ 0xabcd) as u8;
            noise_img.put_pixel(x, y, image::Rgba([b(0), b(1), b(2), 255]));
        }
    }

    // Smooth pattern with no noise: the hard case, many borders look alike.
    let mut pattern = RgbaImage::new(n, n);
    for x in 0..n {
        for y in 0..n {
            pattern.put_pixel(x, y, image::Rgba([x as u8, y as u8, (x ^ y) as u8, 255]));
        }
    }

    vec![
        Case { name: "synth-gradient".into(), img: grad },
        Case { name: "synth-iid".into(), img: noise_img },
        Case { name: "synth-pattern".into(), img: pattern },
    ]
}

/// Position `q` of the encoded image holds original tile `perm[q]`.
fn permutation(n: usize, trial: usize) -> Vec<usize> {
    let mut perm: Vec<usize> = (0..n).collect();
    let mut rng = SplitMix64::seed_from_u64(trial as u64 + 1);
    perm.shuffle(&mut rng);
    perm
}

fn shuffle_tiles(img: &RgbaImage, s: u32, perm: &[usize]) -> RgbaImage {
    let (w, h) = img.dimensions();
    let (tw, th) = (w / s, h / s);
    let mut out = RgbaImage::new(w, h);
    for (q, &t) in perm.iter().enumerate() {
        let q = q as u32;
        let t = t as u32;
        let (qx, qy) = (q % s * tw, q / s * th);
        let (tx, ty) = (t % s * tw, t / s * th);
        for dy in 0..th {
            for dx in 0..tw {
                out.put_pixel(qx + dx, qy + dy, *img.get_pixel(tx + dx, ty + dy));
            }
        }
    }
    out
}

/// Raw bytes of each tile, in row-major tile order.
fn tile_bytes(img: &RgbaImage, s: u32) -> Vec<Vec<u8>> {
    let (w, _) = img.dimensions();
    let (tw, th) = (w / s, img.height() / s);
    let raw = img.as_raw();
    (0..s * s)
        .map(|t| {
            let (tx, ty) = (t % s * tw, t / s * th);
            let mut bytes = Vec::with_capacity((tw * th * 4) as usize);
            for dy in 0..th {
                let start = (((ty + dy) * w + tx) * 4) as usize;
                bytes.extend_from_slice(&raw[start..start + (tw * 4) as usize]);
            }
            bytes
        })
        .collect()
}

#[derive(Default)]
struct Score {
    pos_ok: usize,
    tiles: usize,
    nb_ok: usize,
    nb_total: usize,
}

/// `orig` and `dec` are row-major tile lists. A decoded tile is matched to its
/// original id by content, so neighbor checks use original ids:
/// the right neighbor of id `a` is `a + 1` and the lower neighbor is `a + s`.
fn score(orig: &[Vec<u8>], dec: &[Vec<u8>], s: usize) -> Score {
    let ids: HashMap<&[u8], usize> = orig.iter().enumerate().map(|(i, b)| (b.as_slice(), i)).collect();
    let id_of: Vec<Option<usize>> = dec.iter().map(|b| ids.get(b.as_slice()).copied()).collect();

    let mut sc = Score { tiles: s * s, ..Default::default() };
    sc.pos_ok = (0..s * s).filter(|&p| dec[p] == orig[p]).count();

    for r in 0..s {
        for c in 0..s {
            let p = r * s + c;
            if c + 1 < s {
                sc.nb_total += 1;
                if let (Some(a), Some(b)) = (id_of[p], id_of[p + 1]) {
                    sc.nb_ok += (b == a + 1) as usize;
                }
            }
            if r + 1 < s {
                sc.nb_total += 1;
                if let (Some(a), Some(b)) = (id_of[p], id_of[p + s]) {
                    sc.nb_ok += (b == a + s) as usize;
                }
            }
        }
    }
    sc
}

/// FNV-1a over the decoded pixels, to detect any change in output.
fn fnv1a(data: &[u8], mut h: u64) -> u64 {
    for &b in data {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

fn main() {
    let mut splits: Vec<u32> = vec![4, 8, 16, 32];
    let mut trials: usize = 3;
    let mut files: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--splits" => {
                splits = args
                    .next()
                    .expect("--splits needs a value")
                    .split(',')
                    .map(|v| v.parse().expect("split must be an integer"))
                    .collect();
            }
            "--trials" => {
                trials = args.next().expect("--trials needs a value").parse().expect("trials must be an integer");
            }
            _ => files.push(arg),
        }
    }

    let mut cases = synthetic_cases();
    for path in &files {
        let img = image::open(path).unwrap_or_else(|e| panic!("failed to open {path}: {e}")).to_rgba8();
        let name = std::path::Path::new(path)
            .file_name()
            .map_or_else(|| path.clone(), |n| n.to_string_lossy().into_owned());
        cases.push(Case { name, img });
    }

    println!(
        "{:<18} {:>5} {:>6} {:>8} {:>8} {:>7} {:>10}  {:<16}",
        "case", "split", "trials", "pos%", "nb%", "exact", "decode_ms", "hash"
    );

    for case in &cases {
        let (w, h) = case.img.dimensions();
        for &s in &splits {
            if w % s != 0 || h % s != 0 {
                println!("{:<18} {:>5} skipped: {w}x{h} is not divisible by {s}", case.name, s);
                continue;
            }
            let s_usize = s as usize;
            let orig_tiles = tile_bytes(&case.img, s);
            let split = NonZeroU32::new(s).expect("split must be non-zero");

            let mut total = Score::default();
            let mut exact = 0;
            let mut total_ms = 0.0;
            let mut hash = 0xcbf2_9ce4_8422_2325_u64;

            for trial in 0..trials {
                let perm = permutation(s_usize * s_usize, trial);
                let encoded = shuffle_tiles(&case.img, s, &perm);

                let start = Instant::now();
                let decoded = slice_n_shuffle::decode_image(&encoded, split, split)
                    .expect("decode_image failed");
                total_ms += start.elapsed().as_secs_f64() * 1000.0;

                hash = fnv1a(decoded.as_raw(), hash);
                exact += (decoded.as_raw() == case.img.as_raw()) as usize;

                let sc = score(&orig_tiles, &tile_bytes(&decoded, s), s_usize);
                total.pos_ok += sc.pos_ok;
                total.tiles += sc.tiles;
                total.nb_ok += sc.nb_ok;
                total.nb_total += sc.nb_total;
            }

            println!(
                "{:<18} {:>5} {:>6} {:>7.1}% {:>7.1}% {:>3}/{:<3} {:>10.1}  {:016x}",
                case.name,
                s,
                trials,
                100.0 * total.pos_ok as f64 / total.tiles as f64,
                100.0 * total.nb_ok as f64 / total.nb_total as f64,
                exact,
                trials,
                total_ms / trials as f64,
                hash,
            );
        }
    }
}
