use crate::W;

pub struct Ember {
    pub x: f32,
    pub y: f32,
    pub vx: f32,
    pub vy: f32,
    pub heat: u16,
    pub cool: u16, // per-frame heat retention, /256
}

/// 3x3 stamp with (1, 1/2, 1/4) falloff. Caller guarantees the position is
/// at least one cell away from every edge.
pub fn splat(buf: &mut [u16], idx: usize, heat: u16) {
    buf[idx] = buf[idx].saturating_add(heat);
    for d in [idx - 1, idx + 1, idx - W, idx + W] {
        buf[d] = buf[d].saturating_add(heat >> 1);
    }
    for d in [idx - W - 1, idx - W + 1, idx + W - 1, idx + W + 1] {
        buf[d] = buf[d].saturating_add(heat >> 2);
    }
}
