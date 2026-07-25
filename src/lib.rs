mod bloom;
mod embers;
mod fire;
mod noise;
mod palette;
mod rng;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use fire::Fire;

// Simulation runs at classic doom-fire resolution; the heat field is
// bilinearly upscaled 3x at display time (palette lookup per output pixel).
pub const W: usize = 320;
pub const H: usize = 200;
const SCALE: usize = 3;
pub const OW: usize = W * SCALE;
pub const OH: usize = H * SCALE;

// Bloom works on a half-res bright-pass of the display buffer.
const BW: usize = W / 2;
const BH: usize = H / 2;
