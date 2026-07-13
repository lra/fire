// FIRE_SNAPSHOT=dir renders a few frames to PPM and exits.

pub const SNAP_FRAMES: [u32; 5] = [150, 240, 320, 324, 328];

pub fn write_ppm(path: &str, buf: &[u32], w: usize, h: usize) -> std::io::Result<()> {
    let mut bytes = Vec::with_capacity(w * h * 3 + 20);
    bytes.extend_from_slice(format!("P6\n{w} {h}\n255\n").as_bytes());
    for &px in buf {
        bytes.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, px as u8]);
    }
    std::fs::write(path, bytes)
}
