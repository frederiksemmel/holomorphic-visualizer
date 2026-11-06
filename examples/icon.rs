//! Draws the app icon: what a shared link shows, without the window around it,
//! as white lines on black. See web/icons/render.sh.
//!
//!     cargo run --example icon -- <out.png> <size> <line width> <fill> <fragment>
//!
//! The picture is a square of 512 screen points, and the line width is in those
//! (the app draws 1.5). The image is centred, and scaled until its longer side
//! takes up the fraction `fill` of the picture. The geometry follows `vertex` in
//! src/grid.wgsl.

use std::{env, fs::File, io::BufWriter};

use conformal_visualizer::{GridLayout, expr, scene::Scene};
use num::Complex;

/// Points across the picture.
const WINDOW: f64 = 512.0;
/// Samples per pixel along each axis.
const SAMPLES: usize = 3;
/// As in src/main.rs.
const MAX_SEGMENTS: u32 = 1_000_000;
const MAX_DENSITY: f64 = 2048.0;

fn dot(u: Complex<f64>, v: Complex<f64>) -> f64 {
    u.re * v.re + u.im * v.im
}

/// A value in linear light as an sRGB byte.
fn encode(linear: f32) -> u8 {
    let srgb = if linear <= 0.003_130_8 {
        linear * 12.92
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0 + 0.5) as u8
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let [out, size, line_width, fill, fragment] = args.as_slice() else {
        panic!("usage: icon <out.png> <size> <line width> <fill> <fragment>");
    };
    let size: usize = size.parse().expect("size");
    let line_width: f64 = line_width.parse().expect("line width");
    let fill: f64 = fill.parse().expect("fill");
    let scene = Scene::from_fragment(fragment);
    let f = expr::parse(&scene.formula).expect("formula");
    let p = scene.parameter;
    let [x_min, x_max, y_min, y_max] = scene.grid;
    let layout =
        GridLayout::new(scene.resolution, x_min, x_max, y_min, y_max, MAX_DENSITY, MAX_SEGMENTS);

    let (mut low, mut high) = (Complex::new(f64::MAX, f64::MAX), Complex::new(f64::MIN, f64::MIN));
    for i in 0..layout.segment_count() {
        let w = f.eval(layout.segment(i).0, p);
        if w.norm() < 1e4 {
            low = Complex::new(low.re.min(w.re), low.im.min(w.im));
            high = Complex::new(high.re.max(w.re), high.im.max(w.im));
        }
    }
    let ppu = fill * WINDOW / (high.re - low.re).max(high.im - low.im);
    let offset = -(low + high) / 2.0;
    let to_screen = |w: Complex<f64>| (w + offset) * ppu;

    let n = size * SAMPLES;
    let samples_per_point = n as f64 / WINDOW;
    let mut covered = vec![false; n * n];
    let half = line_width / 2.0;

    for i in 0..layout.segment_count() {
        let (start, finish) = layout.segment(i);
        let (a, b) = (to_screen(f.eval(start, p)), to_screen(f.eval(finish, p)));
        if !(a.norm() < 1e5 && b.norm() < 1e5) {
            continue;
        }
        // Drop segments that jump across a pole instead of following the curve.
        let along = (finish - start) * ppu;
        let tangent_start = f.derivative(start, p) * along;
        let tangent_end = f.derivative(finish, p) * along;
        let chord_vec = b - a;
        let chord = chord_vec.norm();
        let forward = dot(chord_vec, tangent_start) >= 0.0 && dot(chord_vec, tangent_end) >= 0.0;
        let expected = tangent_start.norm().max(tangent_end.norm());
        if !((forward || chord < 4.0 * line_width) && chord < 4.0 * expected + line_width) {
            continue;
        }

        // The screen point (0, 0) is the middle of the picture, y up.
        let range = |low: f64, high: f64| {
            let to_sample = |v: f64| (v + WINDOW / 2.0) * samples_per_point;
            let first = to_sample(low - half).floor().max(0.0) as usize;
            let last = to_sample(high + half).ceil().min(n as f64).max(0.0) as usize;
            first..last
        };
        let to_point = |sample: usize| (sample as f64 + 0.5) / samples_per_point - WINDOW / 2.0;
        for row in range(a.im.min(b.im), a.im.max(b.im)) {
            for column in range(a.re.min(b.re), a.re.max(b.re)) {
                let to = Complex::new(to_point(column), to_point(row)) - a;
                let t = if chord > 1e-9 {
                    (dot(to, chord_vec) / (chord * chord)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if (to - chord_vec * t).norm_sqr() <= half * half {
                    covered[(n - 1 - row) * n + column] = true;
                }
            }
        }
    }

    // Averaged in linear light, like the app's multisampling.
    let mut pixels = Vec::with_capacity(size * size * 3);
    for row in 0..size {
        for column in 0..size {
            let hits = (0..SAMPLES * SAMPLES)
                .filter(|k| {
                    covered[(row * SAMPLES + k / SAMPLES) * n
                        + column * SAMPLES
                        + k % SAMPLES]
                })
                .count();
            let value = encode(hits as f32 / (SAMPLES * SAMPLES) as f32);
            pixels.extend([value; 3]);
        }
    }

    let file = BufWriter::new(File::create(out).expect("output file"));
    let mut encoder = png::Encoder::new(file, size as u32, size as u32);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder.write_header().expect("png header").write_image_data(&pixels).expect("png data");
}
