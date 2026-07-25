// Plain `extern "C"` exports — no wasm-bindgen, no npm. web/index.html
// instantiates the module directly and blits the RGBA buffer to a canvas.

use crate::{Fire, OH, OW};

pub struct WasmFire {
    fire: Fire,
    rgba: Vec<u8>,
}

#[unsafe(no_mangle)]
pub extern "C" fn fire_new() -> *mut WasmFire {
    Box::into_raw(Box::new(WasmFire {
        fire: Fire::new(),
        rgba: vec![0; OW * OH * 4],
    }))
}

/// Advance one frame; returns a pointer to OW*OH*4 RGBA bytes in wasm memory.
#[unsafe(no_mangle)]
pub extern "C" fn fire_frame(p: *mut WasmFire) -> *const u8 {
    let wf = unsafe { &mut *p };
    let screen = wf.fire.step();
    for (px, out) in screen.iter().zip(wf.rgba.chunks_exact_mut(4)) {
        out[0] = (px >> 16) as u8;
        out[1] = (px >> 8) as u8;
        out[2] = *px as u8;
        out[3] = 255;
    }
    wf.rgba.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn fire_width() -> usize {
    OW
}

#[unsafe(no_mangle)]
pub extern "C" fn fire_height() -> usize {
    OH
}
