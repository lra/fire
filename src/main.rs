use minifb::{Key, Scale, ScaleMode, Window, WindowOptions};

// Simulation runs at classic doom-fire resolution; the heat field is
// bilinearly upscaled 3x at display time (palette lookup per output pixel).
const W: usize = 320;
const H: usize = 200;
const SCALE: usize = 3;
const OW: usize = W * SCALE;
const OH: usize = H * SCALE;

// Bloom works on a half-res bright-pass of the display buffer.
const BW: usize = W / 2;
const BH: usize = H / 2;

/// 64-bit LCG, returning the (well-mixed) high 32 bits.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 32) as u32
    }
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / 16_777_216.0
    }
}

// ── Palette ──────────────────────────────────────────────────────────────
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
fn make_palette() -> Vec<u32> {
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

// ── Value noise ──────────────────────────────────────────────────────────
// One tiling 256x256 texture shared by the cooling map and the heat source.

const NOISE: usize = 256;

fn make_noise(rng: &mut Rng) -> Vec<u8> {
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

// ── Bloom ────────────────────────────────────────────────────────────────

/// Separable 7-tap box blur, horizontal pass. Edges clamp.
fn blur_h(src: &[u16], dst: &mut [u16], w: usize, h: usize) {
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
fn blur_v(src: &[u16], dst: &mut [u16], w: usize, h: usize) {
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

// ── Embers ───────────────────────────────────────────────────────────────

struct Ember {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    heat: u16,
    cool: u16, // per-frame heat retention, /256
}

/// 3x3 stamp with (1, 1/2, 1/4) falloff. Caller guarantees the position is
/// at least one cell away from every edge.
fn splat(buf: &mut [u16], idx: usize, heat: u16) {
    buf[idx] = buf[idx].saturating_add(heat);
    for d in [idx - 1, idx + 1, idx - W, idx + W] {
        buf[d] = buf[d].saturating_add(heat >> 1);
    }
    for d in [idx - W - 1, idx - W + 1, idx + W - 1, idx + W + 1] {
        buf[d] = buf[d].saturating_add(heat >> 2);
    }
}

// ── Snapshot (FIRE_SNAPSHOT=dir renders a few frames to PPM and exits) ───

const SNAP_FRAMES: [u32; 5] = [150, 240, 320, 324, 328];

fn write_ppm(path: &str, buf: &[u32], w: usize, h: usize) -> std::io::Result<()> {
    let mut bytes = Vec::with_capacity(w * h * 3 + 20);
    bytes.extend_from_slice(format!("P6\n{w} {h}\n255\n").as_bytes());
    for &px in buf {
        bytes.extend_from_slice(&[(px >> 16) as u8, (px >> 8) as u8, px as u8]);
    }
    std::fs::write(path, bytes)
}

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
                "Fire  —  ESC to quit",
                OW,
                OH,
                WindowOptions {
                    scale: Scale::X1,
                    scale_mode: ScaleMode::Stretch,
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

    let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
    let palette = make_palette();
    let noise = make_noise(&mut rng);

    // `sim`: fire simulation — recomputed every frame as a bottom-up cascade.
    // `disp`: display buffer — lerps toward `sim` for smooth motion.
    let mut sim = vec![0u16; W * H];
    let mut disp = vec![0u16; W * H];
    let mut hstretch = vec![0u16; OW * H];
    let mut bloom = vec![0u16; BW * BH];
    let mut btmp = vec![0u16; BW * BH];
    let mut screen = vec![0u32; OW * OH];
    let mut embers: Vec<Ember> = Vec::with_capacity(64);

    // Fraction of the gap closed per frame: 1/BLEND.
    const BLEND: u32 = 5;

    let mut frame: u32 = 0;
    // 24.8 fixed-point scroll offsets into the noise tile.
    let mut cool_y_fp: u32 = 0;
    let mut cool_x_fp: i32 = 0;
    let mut src_y_fp: u32 = 0;
    let mut src_x_fp: i32 = 0;
    // Wind phases live in [0, τ) — `sin(frame * k)` would degrade once
    // frame stops being exact in f32 (~78h at 60fps).
    let mut phase_a = 0f32;
    let mut phase_b = 0f32;

    loop {
        if let Some(w) = &window
            && (!w.is_open() || w.is_key_down(Key::Escape))
        {
            break;
        }

        frame = frame.wrapping_add(1);

        // Slow irregular sway, ±1: two incommensurate sines (~6.3s and ~1.6s).
        phase_a = (phase_a + 0.0165) % std::f32::consts::TAU;
        phase_b = (phase_b + 0.0648) % std::f32::consts::TAU;
        let wind = 0.6 * phase_a.sin() + 0.4 * phase_b.sin();
        cool_x_fp = cool_x_fp.wrapping_sub((wind * 64.0) as i32 - 13); // wind + slow drift
        src_x_fp = src_x_fp.wrapping_sub((wind * 96.0) as i32);
        src_y_fp = src_y_fp.wrapping_add(170);
        cool_y_fp = cool_y_fp.wrapping_add(300);

        // Wind shear: sampling from the left makes features drift right as
        // they rise, so raise the left-sample probability with the wind —
        // proportionally to height, keeping the base anchored.
        // Per row: left if top5bits < lshift[y], centre for the next 12/32.
        let mut lshift = [0u32; H];
        for (y, l) in lshift.iter_mut().enumerate() {
            let skew = wind * 6.0 * ((H - 1 - y) as f32 / H as f32);
            *l = (10.0 + skew).clamp(2.0, 18.0) as u32;
        }

        // ── Heat source ──────────────────────────────────────────────────
        // Bottom three rows: a bed that surges and subsides. A scrolling
        // slice of the noise tile gives bright regions and dim lulls that
        // travel along the base instead of uniform max heat with holes.
        let src_row = (((src_y_fp >> 8) as usize) & (NOISE - 1)) << 8;
        let coal_row = (((src_y_fp >> 10) as usize).wrapping_add(97) & (NOISE - 1)) << 8;
        let src_x = (src_x_fp >> 8).rem_euclid(NOISE as i32) as usize;
        let mut bed = [0u32; W];
        for (x, b) in bed.iter_mut().enumerate() {
            let r = rng.next();
            let n = noise[src_row + ((x + src_x) & (NOISE - 1))] as u32;
            // Cap below the palette's bleach zone so bed surges glow
            // yellow-white instead of clipping to a featureless disc.
            let mut heat = (42000 + ((n * 23535) >> 8)).min(63500);
            // Coal structure: a faster-varying, slower-scrolling second
            // noise read breaks the bed out of a flat yellow bar.
            let coal = noise[coal_row + ((x * 5 + 31) & (NOISE - 1))] as u32;
            heat = (heat * (224 + ((coal * 31) >> 8))) >> 8;
            heat -= (r >> 13) & 1023; // fine flicker
            if n < 24 {
                heat /= 2; // ember dips, ~9% of the bed
            }
            if (r >> 26) & 63 == 0 {
                heat /= 2; // a whisper of the classic sparkle holes
            }
            *b = heat;
        }
        // Smooth the bed horizontally so the per-column flicker doesn't
        // read as a comb of vertical hairlines at 3x upscale.
        for x in 0..W {
            let heat = (bed[x.saturating_sub(1)] + 2 * bed[x] + bed[(x + 1).min(W - 1)]) >> 2;
            for row in H - 3..H {
                sim[row * W + x] = heat as u16;
            }
        }

        // ── Propagation ──────────────────────────────────────────────────
        // In-place bottom-up cascade. Each cell pulls a [1,6,1]-weighted
        // average of the row below (billowy rounded tongues) through a
        // randomly wind-shifted kernel (doom-fire liveliness). Decay comes
        // from a smooth scrolling cooling map (coherent rising licks) plus
        // fine jitter, with extra extinction below the ember band so dying
        // wisps snuff crisply instead of smearing into fog.
        let cool_y = (cool_y_fp >> 8) as usize;
        let cool_x = (cool_x_fp >> 8).rem_euclid(NOISE as i32) as usize;
        for y in (0..H - 3).rev() {
            // The cooling field is sampled with y halved — stretching its
            // features x2 vertically into the tall channels that carve the
            // flame body into discrete tapering tongues.
            let nrow = (((y >> 1) + cool_y) & (NOISE - 1)) << 8;
            // Fade the cooling map in over the bottom rows: the body stays
            // connected near the bed, tongues separate higher up.
            let gain = ((H - 4 - y) as u32 * 8).clamp(48, 256);
            // Per-pixel jitter grows with height so tongue tips dissolve
            // into ragged flicker instead of airbrushed cones.
            let jamp = 96 + ((H - 4 - y) as u32 * 352) / (H as u32 - 4);
            // Domain warp: a second, vertically stretched, half-speed noise
            // read bends the cooling map's sample position so lick edges
            // come out sinuous instead of straight-sided cones.
            let wrow = (((y >> 1) + (cool_y >> 1)) & (NOISE - 1)) << 8;
            let left = lshift[y];
            let below = (y + 1) * W;
            for x in 0..W {
                let r = rng.next();
                let v = r >> 27;
                let dir = if v < left {
                    -1
                } else if v < left + 12 {
                    0
                } else {
                    1
                };
                let cx = (x as i32 + dir).clamp(0, W as i32 - 1) as usize;
                let a = sim[below + cx.saturating_sub(1)] as u32;
                let b = sim[below + cx] as u32;
                let c = sim[below + (cx + 1).min(W - 1)] as u32;
                let src = (a + 6 * b + c) >> 3;
                let warp = ((noise[wrow + ((x + 191) & (NOISE - 1))] as i32 - 128) * 3) >> 4;
                let wx = ((x + cool_x + NOISE) as i32 + warp) as usize & (NOISE - 1);
                let cool = noise[nrow + wx] as u32;
                let mut decay = 40 + ((((cool * 600) >> 8) * gain) >> 8) + (((r >> 30) * jamp) >> 2);
                if src < 8192 {
                    decay += 200;
                }
                // Melt heat that piles up against the clamped side borders.
                let border = x.min(W - 1 - x);
                if border < 3 {
                    decay += 180 - 60 * border as u32;
                }
                sim[y * W + x] = src.saturating_sub(decay) as u16;
            }
        }

        // ── Display lerp ─────────────────────────────────────────────────
        for i in 0..W * H {
            let target = sim[i] as u32;
            let current = disp[i] as u32;
            disp[i] = ((current * (BLEND - 1) + target) / BLEND) as u16;
        }

        // ── Embers ───────────────────────────────────────────────────────
        // Sparks that detach and rise. Splatted into the display buffer
        // after the lerp: the stamp stays crisp, and last frame's stamps
        // decay 1/BLEND per frame into a natural fading trail.
        if embers.len() < 64 && (rng.next() >> 28) < 6 {
            let x = 4 + (rng.next() as usize) % (W - 8);
            if sim[(H - 8) * W + x] > 0xC000 {
                embers.push(Ember {
                    x: x as f32,
                    y: (H - 8) as f32,
                    vx: 1.0 * (rng.unit() - 0.5),
                    vy: -(0.5 + 0.9 * rng.unit()),
                    heat: 0xE000,
                    // Most sparks are brief; 1 in 8 is a floater that lives
                    // long enough to sail clear of the flame crown.
                    cool: if rng.next() & 7 == 0 { 252 } else { 246 },
                });
            }
        }
        let mut k = 0;
        while k < embers.len() {
            let e = &mut embers[k];
            e.vx = e.vx * 0.96 + 0.3 * (rng.unit() - 0.5) + wind * 0.02;
            e.vy = (e.vy - 0.008).max(-1.6);
            e.x += e.vx;
            e.y += e.vy;
            e.heat = ((e.heat as u32 * e.cool as u32) >> 8) as u16;
            if e.heat < 0x3400 || e.y < 2.0 || e.x < 1.0 || e.x > (W - 2) as f32 {
                embers.swap_remove(k);
                continue;
            }
            // Stamp the half-step position too, so the trail is a stroke
            // rather than a chain of beads.
            let heat = e.heat;
            let mid = (e.y - e.vy * 0.5) as usize * W + (e.x - e.vx * 0.5) as usize;
            splat(&mut disp, mid, (heat >> 1) + (heat >> 2));
            splat(&mut disp, e.y as usize * W + e.x as usize, heat);
            k += 1;
        }

        // ── Bloom ────────────────────────────────────────────────────────
        // Bright-pass + 2x2 downsample, then two box-blur iterations
        // (≈ gaussian σ~2.4 at half res). Composited additively below.
        for by in 0..BH {
            for bx in 0..BW {
                let i = by * 2 * W + bx * 2;
                let s = disp[i].saturating_sub(0xA000) as u32
                    + disp[i + 1].saturating_sub(0xA000) as u32
                    + disp[i + W].saturating_sub(0xA000) as u32
                    + disp[i + W + 1].saturating_sub(0xA000) as u32;
                bloom[by * BW + bx] = (s >> 2) as u16;
            }
        }
        for _ in 0..2 {
            blur_h(&bloom, &mut btmp, BW, BH);
            blur_v(&btmp, &mut bloom, BW, BH);
        }

        // ── Upscale + palette + bloom ────────────────────────────────────
        // Bilinear on the HEAT FIELD, palette lookup per output pixel:
        // flame fringes traverse the palette (black→red→orange) instead of
        // averaging RGB into muddy browns. Horizontal pass first…
        const W3: [u32; 3] = [0, 85, 171];
        for y in 0..H {
            let srow = y * W;
            let drow = y * OW;
            for ox in 0..OW {
                let sx = ox / SCALE;
                let wx = W3[ox % SCALE];
                let a = disp[srow + sx] as u32;
                let b = disp[srow + (sx + 1).min(W - 1)] as u32;
                hstretch[drow + ox] = ((a * (256 - wx) + b * wx) >> 8) as u16;
            }
        }

        // …then a fused vertical + dither + palette + bloom pass.
        // Ordered dither of ±120 heat units (~1 output LSB in the ember
        // zone) dissolves the remaining 8-bit banding; XOR with the frame
        // rotates the pattern so it averages out temporally.
        const BAYER: [i32; 16] = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5];
        const W6: [u32; 6] = [0, 43, 85, 128, 171, 213];
        let fxor = (frame & 15) as usize;
        for oy in 0..OH {
            let wy = W3[oy % SCALE];
            let r0 = oy / SCALE * OW;
            let r1 = (oy / SCALE + 1).min(H - 1) * OW;
            let wby = W6[oy % (2 * SCALE)];
            let b0 = oy / (2 * SCALE) * BW;
            let b1 = (oy / (2 * SCALE) + 1).min(BH - 1) * BW;
            let drow = oy * OW;
            for ox in 0..OW {
                let h = (hstretch[r0 + ox] as u32 * (256 - wy)
                    + hstretch[r1 + ox] as u32 * wy)
                    >> 8;
                let d = BAYER[(((oy & 3) << 2) | (ox & 3)) ^ fxor] * 16 - 120;
                let mut c = palette[(h as i32 + d).clamp(0, 65535) as usize];

                let bx = ox / (2 * SCALE);
                let bx1 = (bx + 1).min(BW - 1);
                let wbx = W6[ox % (2 * SCALE)];
                let top = bloom[b0 + bx] as u32 * (256 - wbx) + bloom[b0 + bx1] as u32 * wbx;
                let bot = bloom[b1 + bx] as u32 * (256 - wbx) + bloom[b1 + bx1] as u32 * wbx;
                let g = ((top * (256 - wby) + bot * wby) >> 23).min(80);
                if g > 0 {
                    // Warm-tinted additive glow: (1.0, 0.74, 0.43) * g.
                    let r = (((c >> 16) & 255) + g).min(255);
                    let gr = (((c >> 8) & 255) + ((g * 190) >> 8)).min(255);
                    let bl = ((c & 255) + ((g * 110) >> 8)).min(255);
                    c = (r << 16) | (gr << 8) | bl;
                }
                screen[drow + ox] = c;
            }
        }

        if let Some(w) = window.as_mut() {
            w.update_with_buffer(&screen, OW, OH).unwrap();
        }

        if let Some(dir) = &snapshot_dir {
            if SNAP_FRAMES.contains(&frame) {
                let path = format!("{dir}/fire_{frame:04}.ppm");
                if let Err(e) = write_ppm(&path, &screen, OW, OH) {
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
