/// Separable 7-tap box blur, horizontal pass. Edges clamp.
pub fn blur_h(src: &[u16], dst: &mut [u16], w: usize, h: usize) {
    const R: usize = 3;
    for y in 0..h {
        let row = &src[y * w..y * w + w];
        let mut sum = row[0] as u32 * R as u32;
        for k in 0..=R {
            sum += row[k.min(w - 1)] as u32;
        }
        for x in 0..w {
            dst[y * w + x] = (sum / (2 * R as u32 + 1)) as u16;
            sum += row[(x + R + 1).min(w - 1)] as u32;
            sum -= row[x.saturating_sub(R)] as u32;
        }
    }
}

/// Separable 7-tap box blur, vertical pass. Edges clamp.
pub fn blur_v(src: &[u16], dst: &mut [u16], w: usize, h: usize) {
    const R: usize = 3;
    for x in 0..w {
        let mut sum = src[x] as u32 * R as u32;
        for k in 0..=R {
            sum += src[k.min(h - 1) * w + x] as u32;
        }
        for y in 0..h {
            dst[y * w + x] = (sum / (2 * R as u32 + 1)) as u16;
            sum += src[(y + R + 1).min(h - 1) * w + x] as u32;
            sum -= src[y.saturating_sub(R) * w + x] as u32;
        }
    }
}
