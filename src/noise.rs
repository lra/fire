// One tiling 256x256 texture shared by the cooling map and the heat source.

use crate::rng::Rng;

pub const NOISE: usize = 256;

pub fn make_noise(rng: &mut Rng) -> Vec<u8> {
    let mut acc = vec![0f32; NOISE * NOISE];
    for (cell, amp) in [(64usize, 3.0f32), (32, 4.0), (16, 2.0), (8, 1.0)] {
        let n = NOISE / cell;
        let lattice: Vec<f32> = (0..n * n).map(|_| rng.unit()).collect();
        for y in 0..NOISE {
            let fy = (y % cell) as f32 / cell as f32;
            let uy = fy * fy * (3.0 - 2.0 * fy);
            let y0 = (y / cell) % n;
            let y1 = (y0 + 1) % n;
            for x in 0..NOISE {
                let fx = (x % cell) as f32 / cell as f32;
                let ux = fx * fx * (3.0 - 2.0 * fx);
                let x0 = (x / cell) % n;
                let x1 = (x0 + 1) % n;
                let a = lattice[y0 * n + x0];
                let b = lattice[y0 * n + x1];
                let c = lattice[y1 * n + x0];
                let d = lattice[y1 * n + x1];
                let top = a + (b - a) * ux;
                let bot = c + (d - c) * ux;
                acc[y * NOISE + x] += (top + (bot - top) * uy) * amp;
            }
        }
    }
    let min = acc.iter().cloned().fold(f32::MAX, f32::min);
    let max = acc.iter().cloned().fold(f32::MIN, f32::max);
    acc.iter()
        .map(|&v| ((v - min) / (max - min) * 255.0) as u8)
        .collect()
}
