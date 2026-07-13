// Blackbody-styled gradient: black → warm smoke → ember maroon → red →
// orange → amber → yellow → white-hot core.

const STOPS: [(f32, [u8; 3]); 12] = [
    (0.000, [0, 0, 0]),
    (0.030, [10, 6, 5]),
    (0.080, [38, 9, 5]),
    (0.180, [94, 18, 8]),
    (0.320, [170, 34, 6]),
    (0.480, [230, 80, 8]),
    (0.620, [255, 128, 10]),
    (0.750, [255, 180, 24]),
    (0.850, [255, 224, 70]),
    (0.930, [255, 246, 150]),
    (0.980, [255, 252, 222]),
    (1.000, [255, 255, 255]),
];

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// One entry per possible u16 heat value, so the smooth display buffer
/// never shows palette-index banding.
pub fn make_palette() -> Vec<u32> {
    (0..0x10000u32)
        .map(|i| {
            let t = i as f32 / 65535.0;
            // Tone curve: keep white hugging the base, stretch the red tongues.
            let p = t.powf(1.35);

            let hi = STOPS.iter().position(|s| s.0 >= p).unwrap_or(STOPS.len() - 1);
            let lo = hi.saturating_sub(1);
            let (p0, c0) = STOPS[lo];
            let (p1, c1) = STOPS[hi];
            let f = if p1 > p0 { (p - p0) / (p1 - p0) } else { 0.0 };

            // Interpolate in linear light — sRGB lerps go muddy in the oranges.
            let mut rgb = [0f32; 3];
            for ch in 0..3 {
                let a = srgb_to_linear(c0[ch] as f32 / 255.0);
                let b = srgb_to_linear(c1[ch] as f32 / 255.0);
                rgb[ch] = a + (b - a) * f;
            }

            // Filmic bleach: the core saturates channel by channel (R first,
            // then G, then B) like an overexposed camera, instead of a flat
            // yellow→white crossfade.
            if p > 0.65 {
                let s = ((p - 0.65) / 0.35).clamp(0.0, 1.0);
                let w = s * s * (3.0 - 2.0 * s);
                let norm = 1.0 / (1.0 - (-2.6f32).exp());
                for c in rgb.iter_mut() {
                    let tm = (1.0 - (-2.6 * *c).exp()) * norm;
                    *c = *c * (1.0 - w) + tm * w;
                }
            }

            let r = (linear_to_srgb(rgb[0]).clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            let g = (linear_to_srgb(rgb[1]).clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            let b = (linear_to_srgb(rgb[2]).clamp(0.0, 1.0) * 255.0 + 0.5) as u32;
            (r << 16) | (g << 8) | b
        })
        .collect()
}
