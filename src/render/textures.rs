//! Prozedurale Texturen: keine fremden Assets, alles wird beim Start erzeugt.
//! Paneele mit Fugen und Nieten, Gestein, Erzadern, Planetenoberflächen, Nebel.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec3;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

// ---------------------------------------------------------------------------
// Rauschen
// ---------------------------------------------------------------------------

fn hash(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn lattice2(x: i32, y: i32, seed: u32) -> f32 {
    let h = hash(
        (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ seed.wrapping_mul(0xcb1a_b31f),
    );
    (h & 0xffff) as f32 / 65535.0
}

fn lattice3(x: i32, y: i32, z: i32, seed: u32) -> f32 {
    let h = hash(
        (x as u32).wrapping_mul(0x8da6_b343)
            ^ (y as u32).wrapping_mul(0xd816_3841)
            ^ (z as u32).wrapping_mul(0xcb1a_b31f)
            ^ seed.wrapping_mul(0x1656_67b1),
    );
    (h & 0xffff) as f32 / 65535.0
}

fn smooth(t: f32) -> f32 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// Kachelbares Wertrauschen (Periode in Gitterzellen; 0 = nicht kachelbar).
pub fn vnoise2(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let xi = x.floor() as i32;
    let yi = y.floor() as i32;
    let (fx, fy) = (smooth(x - xi as f32), smooth(y - yi as f32));
    let w = |i: i32| if period > 0 { i.rem_euclid(period) } else { i };
    let a = lattice2(w(xi), w(yi), seed);
    let b = lattice2(w(xi + 1), w(yi), seed);
    let c = lattice2(w(xi), w(yi + 1), seed);
    let d = lattice2(w(xi + 1), w(yi + 1), seed);
    let ab = a + (b - a) * fx;
    let cd = c + (d - c) * fx;
    ab + (cd - ab) * fy
}

pub fn fbm2(x: f32, y: f32, octaves: u32, period: i32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut f = 1.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        let p = if period > 0 { period * f as i32 } else { 0 };
        sum += vnoise2(x * f, y * f, p, seed.wrapping_add(o * 101)) * amp;
        norm += amp;
        amp *= 0.5;
        f *= 2.0;
    }
    sum / norm
}

pub fn vnoise3(p: Vec3, seed: u32) -> f32 {
    let (xi, yi, zi) = (p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
    let (fx, fy, fz) = (
        smooth(p.x - xi as f32),
        smooth(p.y - yi as f32),
        smooth(p.z - zi as f32),
    );
    let l = |dx, dy, dz| lattice3(xi + dx, yi + dy, zi + dz, seed);
    let x00 = l(0, 0, 0) + (l(1, 0, 0) - l(0, 0, 0)) * fx;
    let x10 = l(0, 1, 0) + (l(1, 1, 0) - l(0, 1, 0)) * fx;
    let x01 = l(0, 0, 1) + (l(1, 0, 1) - l(0, 0, 1)) * fx;
    let x11 = l(0, 1, 1) + (l(1, 1, 1) - l(0, 1, 1)) * fx;
    let y0 = x00 + (x10 - x00) * fy;
    let y1 = x01 + (x11 - x01) * fy;
    y0 + (y1 - y0) * fz
}

pub fn fbm3(p: Vec3, octaves: u32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut f = 1.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += vnoise3(p * f, seed.wrapping_add(o * 131)) * amp;
        norm += amp;
        amp *= 0.5;
        f *= 2.03;
    }
    sum / norm
}

// ---------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------

pub type Rgb = [f32; 3];

fn to_u8(x: f32) -> u8 {
    (x.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

pub fn lerp3(a: Rgb, b: Rgb, t: f32) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Farbverlauf über mehrere Stützfarben, t in 0..1.
pub fn gradient(colors: &[Rgb], t: f32) -> Rgb {
    if colors.is_empty() {
        return [1.0; 3];
    }
    if colors.len() == 1 {
        return colors[0];
    }
    let t = t.clamp(0.0, 1.0) * (colors.len() - 1) as f32;
    let i = (t.floor() as usize).min(colors.len() - 2);
    lerp3(colors[i], colors[i + 1], t - i as f32)
}

fn sampler(repeat: bool, mips: bool) -> ImageSampler {
    let mode = if repeat {
        ImageAddressMode::Repeat
    } else {
        ImageAddressMode::ClampToEdge
    };
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: if mips {
            ImageFilterMode::Linear
        } else {
            ImageFilterMode::Nearest
        },
        anisotropy_clamp: if mips { 8 } else { 1 },
        ..Default::default()
    })
}

/// RGBA8-Bild, optional mit auf der CPU erzeugten Mipmaps.
pub fn make_image(w: u32, h: u32, data: Vec<u8>, srgb: bool, repeat: bool, mips: bool) -> Image {
    let format = if srgb {
        TextureFormat::Rgba8UnormSrgb
    } else {
        TextureFormat::Rgba8Unorm
    };
    let mut levels = 1u32;
    let mut all = data.clone();
    if mips {
        let (mut cw, mut ch, mut cur) = (w, h, data);
        while cw > 1 && ch > 1 {
            let (nw, nh) = (cw / 2, ch / 2);
            let mut next = vec![0u8; (nw * nh * 4) as usize];
            for y in 0..nh {
                for x in 0..nw {
                    for c in 0..4 {
                        let s = |xx: u32, yy: u32| cur[((yy * cw + xx) * 4 + c) as usize] as u32;
                        let v = s(2 * x, 2 * y)
                            + s(2 * x + 1, 2 * y)
                            + s(2 * x, 2 * y + 1)
                            + s(2 * x + 1, 2 * y + 1);
                        next[((y * nw + x) * 4 + c) as usize] = (v / 4) as u8;
                    }
                }
            }
            all.extend_from_slice(&next);
            cur = next;
            cw = nw;
            ch = nh;
            levels += 1;
        }
    }
    let mut img = Image::new_uninit(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        format,
        RenderAssetUsages::RENDER_WORLD,
    );
    img.texture_descriptor.mip_level_count = levels;
    img.data = Some(all);
    img.sampler = sampler(repeat, mips);
    img
}

/// Normalen aus einer Höhenkarte (kachelbar).
fn normals_from_height(hgt: &[f32], w: usize, h: usize, strength: f32) -> Vec<u8> {
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let s = |xx: isize, yy: isize| {
                let xx = xx.rem_euclid(w as isize) as usize;
                let yy = yy.rem_euclid(h as isize) as usize;
                hgt[yy * w + xx]
            };
            let (xi, yi) = (x as isize, y as isize);
            let dx = (s(xi + 1, yi) - s(xi - 1, yi)) * strength;
            // Bildzeilen laufen nach unten, UV-v ebenfalls → Vorzeichen für +y-up-Normalen.
            let dy = (s(xi, yi - 1) - s(xi, yi + 1)) * strength;
            let n = Vec3::new(-dx, -dy, 1.0).normalize();
            let i = (y * w + x) * 4;
            out[i] = to_u8(n.x * 0.5 + 0.5);
            out[i + 1] = to_u8(n.y * 0.5 + 0.5);
            out[i + 2] = to_u8(n.z * 0.5 + 0.5);
            out[i + 3] = 255;
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Texturen
// ---------------------------------------------------------------------------

pub struct PanelSet {
    pub albedo: Image,
    pub normal: Image,
}

/// Metallpaneele: Fugen, Nieten, leichte Verschmutzung und Kratzer.
pub fn panels(size: u32, panels_per_side: u32, seed: u32) -> PanelSet {
    let n = size as usize;
    let cell = n as f32 / panels_per_side as f32;
    let mut hgt = vec![0.0f32; n * n];
    let mut alb = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let (fx, fy) = (x as f32, y as f32);
            let u = fx / n as f32;
            let v = fy / n as f32;
            let lx = fx % cell;
            let ly = fy % cell;
            let edge = lx.min(cell - lx).min(ly).min(cell - ly);
            // Fuge
            let seam = if edge < 1.6 {
                1.0
            } else if edge < 3.5 {
                1.0 - (edge - 1.6) / 1.9
            } else {
                0.0
            };
            // Nieten an den Ecken
            let mut rivet = 0.0f32;
            for (cx, cy) in [
                (6.5, 6.5),
                (cell - 6.5, 6.5),
                (6.5, cell - 6.5),
                (cell - 6.5, cell - 6.5),
            ] {
                let d = ((lx - cx).powi(2) + (ly - cy).powi(2)).sqrt();
                rivet = rivet.max((1.0 - d / 2.6).clamp(0.0, 1.0));
            }
            let pid = ((fx / cell) as u32) * 7 + (fy / cell) as u32 * 13;
            let panel_tone = (hash(pid.wrapping_add(seed)) & 0xff) as f32 / 255.0;
            let grime = fbm2(u * 8.0, v * 8.0, 5, 8, seed + 3);
            let fine = fbm2(u * 64.0, v * 64.0, 2, 64, seed + 9);
            // Kratzer: feine diagonale Linien aus Rauschen
            let scratch_n = vnoise2(u * 90.0 + v * 30.0, v * 6.0, 0, seed + 17);
            let scratch = ((scratch_n - 0.94) * 16.0).clamp(0.0, 1.0);
            let mut g = 0.80 + panel_tone * 0.10 + (fine - 0.5) * 0.06;
            g -= seam * 0.40;
            g -= ((grime - 0.55) * 0.9).max(0.0) * 0.5 * (0.4 + seam);
            g += scratch * 0.12;
            g -= rivet * 0.12;
            let i = (y * n + x) * 4;
            let c = to_u8(g);
            alb[i] = c;
            alb[i + 1] = c;
            alb[i + 2] = c;
            alb[i + 3] = 255;
            hgt[y * n + x] = -seam * 1.0 + rivet * 0.9 + (fine - 0.5) * 0.08 + panel_tone * 0.15;
        }
    }
    let nrm = normals_from_height(&hgt, n, n, 1.6);
    PanelSet {
        albedo: make_image(size, size, alb, true, true, true),
        normal: make_image(size, size, nrm, false, true, true),
    }
}

/// Leuchtende Fenster (Emissions-Maske) und dunkles Glas.
pub fn windows(size: u32, seed: u32) -> (Image, Image) {
    let n = size as usize;
    let mut emi = vec![0u8; n * n * 4];
    let mut alb = vec![0u8; n * n * 4];
    let cols = 6;
    let rows = 4;
    let cw = n as f32 / cols as f32;
    let ch = n as f32 / rows as f32;
    for y in 0..n {
        for x in 0..n {
            let (cx, cy) = ((x as f32 / cw) as u32, (y as f32 / ch) as u32);
            let lx = x as f32 % cw;
            let ly = y as f32 % ch;
            let inside = lx > cw * 0.18 && lx < cw * 0.82 && ly > ch * 0.25 && ly < ch * 0.75;
            let h = hash(cx * 31 + cy * 97 + seed);
            let lit = (h & 0xff) > 70;
            let warm = (h >> 8) & 1 == 1;
            let i = (y * n + x) * 4;
            let frame = if inside { 0.10 } else { 0.55 };
            alb[i] = to_u8(frame);
            alb[i + 1] = to_u8(frame);
            alb[i + 2] = to_u8(frame + 0.03);
            alb[i + 3] = 255;
            if inside && lit {
                let grad = 0.75 + 0.25 * (1.0 - ly / ch);
                let c: Rgb = if warm {
                    [1.0, 0.82, 0.55]
                } else {
                    [0.55, 0.95, 1.0]
                };
                emi[i] = to_u8(c[0] * grad);
                emi[i + 1] = to_u8(c[1] * grad);
                emi[i + 2] = to_u8(c[2] * grad);
            }
            emi[i + 3] = 255;
        }
    }
    (
        make_image(size, size, alb, true, true, true),
        make_image(size, size, emi, true, true, true),
    )
}

/// Gestein (kachelbar) + Normalen.
pub fn rock(size: u32, seed: u32) -> PanelSet {
    let n = size as usize;
    let mut hgt = vec![0.0f32; n * n];
    let mut alb = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / n as f32;
            let v = y as f32 / n as f32;
            let base = fbm2(u * 6.0, v * 6.0, 6, 6, seed);
            let cr = vnoise2(u * 12.0, v * 12.0, 12, seed + 5);
            let crack = (1.0 - ((cr - 0.5).abs() * 18.0)).clamp(0.0, 1.0);
            let g = 0.45 + base * 0.45 - crack * 0.18;
            let i = (y * n + x) * 4;
            alb[i] = to_u8(g);
            alb[i + 1] = to_u8(g * 0.97);
            alb[i + 2] = to_u8(g * 0.94);
            alb[i + 3] = 255;
            hgt[y * n + x] = base * 1.4 - crack * 0.6;
        }
    }
    let nrm = normals_from_height(&hgt, n, n, 3.0);
    PanelSet {
        albedo: make_image(size, size, alb, true, true, true),
        normal: make_image(size, size, nrm, false, true, true),
    }
}

/// Leuchtende Erzadern (Emissions-Maske, weiß auf schwarz).
pub fn veins(size: u32, seed: u32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / n as f32;
            let v = y as f32 / n as f32;
            let r = fbm2(u * 5.0, v * 5.0, 4, 5, seed);
            let ridge = (1.0 - ((r - 0.5).abs() * 14.0)).clamp(0.0, 1.0).powf(1.5);
            let spots = ((fbm2(u * 9.0, v * 9.0, 3, 9, seed + 7) - 0.62) * 6.0).clamp(0.0, 1.0);
            let m = ridge.max(spots);
            let i = (y * n + x) * 4;
            out[i] = to_u8(m);
            out[i + 1] = to_u8(m);
            out[i + 2] = to_u8(m);
            out[i + 3] = 255;
        }
    }
    make_image(size, size, out, true, true, true)
}

/// Planetenoberfläche (äquirektangulär), nahtlos über 3D-Rauschen auf der Kugel.
pub fn planet(colors: &[Rgb], seed: u32, w: u32, h: u32, bands: f32) -> Image {
    let (wu, hu) = (w as usize, h as usize);
    let mut out = vec![0u8; wu * hu * 4];
    for y in 0..hu {
        let lat = (y as f32 + 0.5) / hu as f32 * std::f32::consts::PI;
        for x in 0..wu {
            let lon = (x as f32 + 0.5) / wu as f32 * std::f32::consts::TAU;
            let p = Vec3::new(lat.sin() * lon.cos(), lat.cos(), lat.sin() * lon.sin());
            let warp = Vec3::new(
                fbm3(p * 1.7 + Vec3::splat(3.1), 4, seed + 11),
                fbm3(p * 1.7 + Vec3::splat(7.7), 4, seed + 12),
                fbm3(p * 1.7 + Vec3::splat(1.3), 4, seed + 13),
            );
            let n = fbm3(p * 2.2 + warp * 1.8, 6, seed);
            let n = ((n - 0.3) / 0.42).clamp(0.0, 1.0);
            let band = ((p.y * bands + n * 3.0 + warp.x * 2.0).sin() * 0.5 + 0.5)
                * (bands / 12.0).min(1.0);
            let t = (n * 0.8 + band * 0.3).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            let mut c = gradient(colors, t);
            // Feine Details und Krater/Flecken
            let detail = fbm3(p * 14.0, 3, seed + 31) - 0.5;
            let k = 1.0 + detail * 0.22;
            c = [c[0] * k, c[1] * k, c[2] * k];
            let crater = ((fbm3(p * 9.0, 3, seed + 21) - 0.66) * 5.0).clamp(0.0, 1.0);
            c = lerp3(c, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6], crater * 0.7);
            let i = (y * wu + x) * 4;
            out[i] = to_u8(c[0]);
            out[i + 1] = to_u8(c[1]);
            out[i + 2] = to_u8(c[2]);
            out[i + 3] = 255;
        }
    }
    make_image(w, h, out, true, false, true)
}

/// Planetenscheibe für die Frontalansicht: UV = planare Projektion der vorderen Halbkugel.
/// Keine Naht, keine Pole – die Rückseite sieht man nie.
pub fn planet_disc(colors: &[Rgb], seed: u32, size: u32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let px = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let py = 1.0 - (y as f32 + 0.5) / n as f32 * 2.0;
            let r2 = (px * px + py * py).min(0.999);
            let s = 1.0 / (px * px + py * py).sqrt().max(1.0);
            let p = Vec3::new(px * s, py * s, (1.0 - r2).sqrt());
            let warp = Vec3::new(
                fbm3(p * 1.7 + Vec3::splat(3.1), 4, seed + 11),
                fbm3(p * 1.7 + Vec3::splat(7.7), 4, seed + 12),
                fbm3(p * 1.7 + Vec3::splat(1.3), 4, seed + 13),
            );
            let nn = fbm3(p * 2.4 + warp * 1.8, 7, seed);
            let nn = ((nn - 0.3) / 0.42).clamp(0.0, 1.0);
            let band = ((p.y * 5.0 + nn * 3.0 + warp.x * 2.0).sin() * 0.5 + 0.5) * 0.5;
            let t = (nn * 0.8 + band * 0.3).clamp(0.0, 1.0);
            let t = t * t * (3.0 - 2.0 * t);
            let mut c = gradient(colors, t);
            let detail = fbm3(p * 22.0, 3, seed + 31) - 0.5;
            let k = 1.0 + detail * 0.25;
            c = [c[0] * k, c[1] * k, c[2] * k];
            let crater = ((fbm3(p * 11.0, 3, seed + 21) - 0.66) * 5.0).clamp(0.0, 1.0);
            c = lerp3(c, [c[0] * 0.55, c[1] * 0.55, c[2] * 0.6], crater * 0.7);
            let i = (y * n + x) * 4;
            out[i] = to_u8(c[0]);
            out[i + 1] = to_u8(c[1]);
            out[i + 2] = to_u8(c[2]);
            out[i + 3] = 255;
        }
    }
    make_image(size, size, out, true, false, true)
}

/// Nebelwolke als Graustufen-Maske mit Alpha (wird über das Material eingefärbt).
pub fn nebula(size: u32, seed: u32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let u = x as f32 / n as f32 - 0.5;
            let v = y as f32 / n as f32 - 0.5;
            let r = (u * u + v * v).sqrt() * 2.0;
            let falloff = (1.0 - r).clamp(0.0, 1.0).powf(1.6);
            let wx = fbm2(u * 3.0 + 10.0, v * 3.0, 4, 0, seed + 1);
            let wy = fbm2(u * 3.0, v * 3.0 + 10.0, 4, 0, seed + 2);
            let d = fbm2(u * 4.0 + wx * 2.5, v * 4.0 + wy * 2.5, 6, 0, seed);
            let wisps = ((d - 0.34) * 2.2).clamp(0.0, 1.0);
            let a = ((wisps * 0.85 + 0.15 * d) * falloff).clamp(0.0, 1.0);
            let bright = 0.55 + 0.45 * fbm2(u * 12.0, v * 12.0, 3, 0, seed + 5);
            let i = (y * n + x) * 4;
            let g = to_u8(bright * a);
            out[i] = g;
            out[i + 1] = g;
            out[i + 2] = g;
            out[i + 3] = to_u8(a);
        }
    }
    make_image(size, size, out, true, false, false)
}

/// Weicher, runder Lichtfleck.
pub fn glow(size: u32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let r = (u * u + v * v).sqrt();
            let a = (1.0 - r).clamp(0.0, 1.0);
            let a = a * a * (0.6 + 0.4 * a);
            let i = (y * n + x) * 4;
            out[i] = 255;
            out[i + 1] = 255;
            out[i + 2] = 255;
            out[i + 3] = to_u8(a);
        }
    }
    make_image(size, size, out, true, false, false)
}

/// Ring (für Schockwellen, Schild, Markierungen).
pub fn ring(size: u32, thickness: f32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let r = (u * u + v * v).sqrt();
            let d = ((r - (1.0 - thickness)) / thickness).clamp(-1.0, 1.0);
            let a = if r > 1.0 {
                0.0
            } else {
                (1.0 - d.abs()).powf(1.5)
            };
            let inner = if r < 1.0 - thickness {
                (r / (1.0 - thickness)).powf(6.0) * 0.25
            } else {
                0.0
            };
            let i = (y * n + x) * 4;
            out[i] = 255;
            out[i + 1] = 255;
            out[i + 2] = 255;
            out[i + 3] = to_u8(a.max(inner));
        }
    }
    make_image(size, size, out, true, false, false)
}

/// Gravitationsanomalie: graue Scheibe, zur Mitte hin dunkler, schwarzer Ring.
pub fn anomaly(size: u32, seed: u32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let u = (x as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let v = (y as f32 + 0.5) / n as f32 * 2.0 - 1.0;
            let r = (u * u + v * v).sqrt();
            let ang = v.atan2(u);
            let swirl = fbm2(ang * 3.0 + r * 9.0, r * 4.0, 4, 0, seed);
            let g = (r.powf(0.8) * 0.75 + (swirl - 0.5) * 0.12).clamp(0.0, 1.0);
            let core_ring = (1.0 - ((r - 0.14) / 0.08).abs()).clamp(0.0, 1.0);
            let shade = g * (1.0 - core_ring * 0.9);
            let edge = ((1.0 - r) * 14.0).clamp(0.0, 1.0);
            let i = (y * n + x) * 4;
            let c = to_u8(shade * 0.62);
            out[i] = c;
            out[i + 1] = c;
            out[i + 2] = to_u8(shade * 0.66);
            out[i + 3] =
                to_u8(edge * (0.55 + 0.45 * (1.0 - r)).min(1.0) * if r < 0.08 { 0.0 } else { 1.0 });
        }
    }
    make_image(size, size, out, true, false, false)
}

/// Warnstreifen für Landeplattformen (Emission).
pub fn stripes(size: u32) -> Image {
    let n = size as usize;
    let mut out = vec![0u8; n * n * 4];
    for y in 0..n {
        for x in 0..n {
            let t = ((x + y) as f32 / n as f32 * 4.0).fract();
            let s = if t < 0.5 { 1.0 } else { 0.18 };
            let i = (y * n + x) * 4;
            out[i] = to_u8(s);
            out[i + 1] = to_u8(s);
            out[i + 2] = to_u8(s);
            out[i + 3] = 255;
        }
    }
    make_image(size, size, out, true, true, true)
}
