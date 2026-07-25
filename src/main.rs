// Native front end: minifb window (or headless FIRE_SNAPSHOT mode) driving
// the shared `fire::Fire` renderer. The wasm front end lives in web/.

#[cfg(target_os = "macos")]
mod fullscreen;
mod snapshot;

use fire::{Fire, OH, OW};
use minifb::{Key, KeyRepeat, Scale, ScaleMode, Window, WindowOptions};
use snapshot::{SNAP_FRAMES, write_ppm};

fn main() {
    let snapshot_dir = std::env::var("FIRE_SNAPSHOT").ok();
    if let Some(dir) = &snapshot_dir
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("snapshot: create {dir}: {e}");
        std::process::exit(1);
    }

    // Snapshot mode is headless: no window, so it works without a display
    // (CI, SSH, remote builds). Interactive mode opens the usual window.
    let mut window = match &snapshot_dir {
        None => {
            let mut w = Window::new(
                "Fire  —  F fullscreen, ESC to quit",
                OW,
                OH,
                WindowOptions {
                    scale: Scale::X1,
                    scale_mode: ScaleMode::AspectRatioStretch,
                    resize: true,
                    ..WindowOptions::default()
                },
            )
            .expect("Could not open window");
            w.set_target_fps(60);
            Some(w)
        }
        Some(_) => None,
    };

    let mut fire = Fire::new();

    // Some(saved windowed frame) while fullscreen, None while windowed.
    #[cfg(target_os = "macos")]
    let mut fs_saved: Option<fullscreen::Saved> = None;

    let mut frame: u32 = 0;
    loop {
        if let Some(w) = window.as_mut() {
            if !w.is_open() || w.is_key_down(Key::Escape) {
                break;
            }
            #[cfg(target_os = "macos")]
            if w.is_key_pressed(Key::F, KeyRepeat::No) {
                fs_saved = match fs_saved.take() {
                    None => Some(fullscreen::enter(w)),
                    Some(s) => {
                        fullscreen::exit(w, &s);
                        None
                    }
                };
            }
        }

        frame = frame.wrapping_add(1);
        let screen = fire.step();

        if let Some(w) = window.as_mut() {
            w.update_with_buffer(screen, OW, OH).unwrap();
        }

        if let Some(dir) = &snapshot_dir {
            if SNAP_FRAMES.contains(&frame) {
                let path = format!("{dir}/fire_{frame:04}.ppm");
                if let Err(e) = write_ppm(&path, screen, OW, OH) {
                    eprintln!("snapshot {path} failed: {e}");
                    std::process::exit(1);
                }
            }
            if frame >= SNAP_FRAMES[SNAP_FRAMES.len() - 1] {
                break;
            }
        }
    }
}
