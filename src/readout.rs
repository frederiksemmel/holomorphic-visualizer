//! What landed under the cursor: the points of the grid that the map sends
//! to a point of the plane, and how to write them down.
//!
//! The map is passed in as `map` and `derivative`, the CPU copy of what the
//! grid shader draws (see [`crate::expr::Function::eval`]).

use crate::axes::with_minus_signs;
use num::Complex;

/// Samples per side of the grid domain that seed the search.
const SAMPLES: usize = 64;
/// A sample seeds Newton's method if, going by the map's derivative there,
/// the point it looks for lies within this many sample cells of it.
const SEED_REACH: f32 = 2.0;
/// Most runs of Newton's method, from the most promising seeds.
const MAX_RUNS: usize = 32;
/// Most steps per run: a few reach a simple root; a double root, where
/// Newton only halves the distance each step, takes more.
const NEWTON_STEPS: usize = 40;
/// Longest Newton step, in sample cells. Near a critical point Newton can
/// throw the point across the plane; the other seeds cover what is far away.
const MAX_NEWTON_STEP: f32 = 2.0;
/// Significant digits an f32 holds; positions are f32, so more would be noise.
const F32_DIGITS: i32 = 7;

/// The points z of the grid domain `[min, max]` that `map` sends to `w`, at
/// most `max_count`, closest to w first; and how many there were in all.
/// `tolerance` is how close map(z) must come to w, in plane units.
///
/// Seeds come from a coarse sample of the domain whose images land near w;
/// Newton's method on map(z) − w takes each to a root. Roots found from
/// several seeds are counted once.
pub fn preimages(
    map: impl Fn(Complex<f32>) -> Complex<f32>,
    derivative: impl Fn(Complex<f32>) -> Complex<f32>,
    (min, max): (Complex<f32>, Complex<f32>),
    w: Complex<f32>,
    tolerance: f32,
    max_count: usize,
) -> (Vec<Complex<f32>>, usize) {
    let size = max - min;
    if !(size.re > 0.0 && size.im > 0.0) {
        return (Vec::new(), 0);
    }
    let cell = size / SAMPLES as f32;
    let cell_size = cell.norm();

    // Each sample's guess at the root by the map's derivative there, and how
    // many cells away that guess is.
    let mut seeds: Vec<(f32, Complex<f32>, Complex<f32>)> = (0..SAMPLES * SAMPLES)
        .filter_map(|n| {
            let (i, j) = (n % SAMPLES, n / SAMPLES);
            let z = min + Complex::new((i as f32 + 0.5) * cell.re, (j as f32 + 0.5) * cell.im);
            let step = (map(z) - w) / derivative(z);
            let cells = step.norm() / cell_size;
            (cells <= SEED_REACH).then_some((cells, z, z - step))
        })
        .collect();
    seeds.sort_by(|a, b| a.0.total_cmp(&b.0));

    let inside = |z: Complex<f32>| {
        let slack = 1e-3 * cell_size;
        (min.re - slack..=max.re + slack).contains(&z.re)
            && (min.im - slack..=max.im + slack).contains(&z.im)
    };
    // Every root found, in the domain or just outside it.
    let mut found: Vec<Complex<f32>> = Vec::new();
    let mut runs = 0;
    for &(_, seed, guess) in &seeds {
        // Seeds that point at a root already found would most likely find
        // it again; leave the runs to the others.
        if found.iter().any(|root| (root - guess).norm() < cell_size) {
            continue;
        }
        if runs == MAX_RUNS {
            break;
        }
        runs += 1;
        let max_step = MAX_NEWTON_STEP * cell_size;
        let Some(z) = newton(&map, &derivative, seed, w, tolerance, max_step) else {
            continue;
        };
        // Roots whose images can't be told apart are one: two runs that end
        // at the same root differ by about that much.
        let same = (2.0 * tolerance / derivative(z).norm()).min(cell_size);
        if found.iter().all(|root| (root - z).norm() > same) {
            found.push(z);
        }
    }
    let mut roots: Vec<_> = found.into_iter().filter(|&z| inside(z)).collect();
    roots.sort_by(|a, b| (a - w).norm().total_cmp(&(b - w).norm()));
    let count = roots.len();
    roots.truncate(max_count);
    (roots, count)
}

/// Newton's method on map(z) − w from `z`, with steps of at most
/// `max_step`. It keeps going after map(z) is within `tolerance` of w until
/// the steps vanish, so runs that reach the same root agree closely.
fn newton(
    map: &impl Fn(Complex<f32>) -> Complex<f32>,
    derivative: &impl Fn(Complex<f32>) -> Complex<f32>,
    mut z: Complex<f32>,
    w: Complex<f32>,
    tolerance: f32,
    max_step: f32,
) -> Option<Complex<f32>> {
    for _ in 0..NEWTON_STEPS {
        let mut step = (map(z) - w) / derivative(z);
        if !(step.re.is_finite() && step.im.is_finite()) {
            break;
        }
        if step.norm() > max_step {
            step *= max_step / step.norm();
        }
        z -= step;
        if step.norm() <= 1e-6 * z.norm().max(1.0) {
            break;
        }
    }
    let residual = (map(z) - w).norm();
    (residual <= tolerance).then_some(z)
}

/// Decimals that tell apart points one screen point apart, where one plane
/// unit takes `points_per_unit` points; at most what an f32 of size
/// `magnitude` holds.
pub fn decimals(points_per_unit: f32, magnitude: f32) -> usize {
    let wanted = points_per_unit.max(1.0).log10().ceil() as i32;
    let whole_digits = magnitude.abs().max(1.0).log10().floor() as i32 + 1;
    wanted.min(F32_DIGITS - whole_digits).max(0) as usize
}

/// `z` as x + yi, with `decimals` decimals in each part and real minus signs.
pub fn complex(z: Complex<f32>, decimals: usize) -> String {
    // Adding zero turns −0 into 0, so −0.0001 rounded to "−0.00" can't show.
    let rounded = |v: f32| {
        let scale = 10f64.powi(decimals as i32);
        (v as f64 * scale).round() / scale + 0.0
    };
    let (re, im) = (rounded(z.re), rounded(z.im));
    let sign = if im < 0.0 { '−' } else { '+' };
    with_minus_signs(&format!("{re:.decimals$} {sign} {:.decimals$}i", im.abs()))
}

/// `x` with three significant digits, or as a whole number when larger, for
/// how much a map stretches: 0.431, 2.31, 23.1, 231, 2314.
pub fn significant(x: f32) -> String {
    let whole_digits = x.abs().max(1e-30).log10().floor() as i32 + 1;
    let decimals = (3 - whole_digits).clamp(0, 6) as usize;
    with_minus_signs(&format!("{x:.decimals$}"))
}

/// An angle in radians as whole degrees, with a real minus sign.
pub fn degrees(angle: f32) -> String {
    with_minus_signs(&format!("{:.0}°", angle.to_degrees().round() + 0.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr;

    const TOLERANCE: f32 = 1e-4;

    fn domain(x_min: f32, x_max: f32, y_min: f32, y_max: f32) -> (Complex<f32>, Complex<f32>) {
        (Complex::new(x_min, y_min), Complex::new(x_max, y_max))
    }

    /// The points of `domain` that `formula` (with p = 1) sends to `w`.
    fn sources(
        formula: &str,
        domain: (Complex<f32>, Complex<f32>),
        w: Complex<f32>,
    ) -> Vec<Complex<f32>> {
        let f = expr::parse(formula).unwrap();
        let p = Complex::new(1.0, 0.0);
        preimages(
            |z| f.eval(z, p),
            |z| f.derivative(z, p),
            domain,
            w,
            TOLERANCE,
            10,
        )
        .0
    }

    fn assert_roots(found: &[Complex<f32>], expected: &[Complex<f32>]) {
        assert_eq!(
            found.len(),
            expected.len(),
            "{found:?}, expected {expected:?}"
        );
        for (found, expected) in found.iter().zip(expected) {
            assert!(
                (found - expected).norm() < 1e-3,
                "{found} should be {expected}"
            );
        }
    }

    #[test]
    fn the_identity_sends_w_to_itself() {
        let w = Complex::new(0.3, -0.7);
        assert_roots(&sources("z", domain(-1.0, 1.0, -1.0, 1.0), w), &[w]);
    }

    #[test]
    fn joukowski_has_two_sources() {
        // z + 1/z = 2.5 at z = 2 and z = 1/2; the nearer one comes first.
        let found = sources(
            "z + 1/z",
            domain(-3.0, 3.0, -3.0, 3.0),
            Complex::new(2.5, 0.0),
        );
        assert_roots(&found, &[Complex::new(2.0, 0.0), Complex::new(0.5, 0.0)]);
    }

    #[test]
    fn exp_has_one_source_per_turn_in_the_domain() {
        let z = Complex::new(0.5, 1.0);
        let w = z.exp();
        assert_roots(&sources("exp(z)", domain(-1.0, 1.0, -3.0, 3.0), w), &[z]);
        let below = z - Complex::new(0.0, std::f32::consts::TAU);
        assert_roots(
            &sources("exp(z)", domain(-1.0, 1.0, -7.0, 7.0), w),
            &[z, below],
        );
    }

    #[test]
    fn nothing_lands_outside_the_image() {
        let w = Complex::new(100.0, 0.0);
        assert!(sources("exp(z)", domain(-1.0, 1.0, -3.0, 3.0), w).is_empty());
        // 4 is z^2 at ±2, just outside the domain.
        assert!(sources("z^2", domain(-1.5, 1.5, -1.5, 1.5), Complex::new(4.0, 0.0)).is_empty());
    }

    #[test]
    fn a_double_root_counts_once() {
        let found = sources("z^2", domain(-1.0, 1.0, -1.0, 1.0), Complex::new(0.0, 0.0));
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].norm() < 1e-2);
    }

    #[test]
    fn at_most_max_count_but_all_are_counted() {
        // exp(z) = 1 at 2πik: k = -2..=2 in this domain.
        let f = expr::parse("exp(z)").unwrap();
        let p = Complex::new(1.0, 0.0);
        let (found, count) = preimages(
            |z| f.eval(z, p),
            |z| f.derivative(z, p),
            domain(-1.0, 1.0, -14.0, 14.0),
            Complex::new(1.0, 0.0),
            TOLERANCE,
            3,
        );
        assert_eq!((found.len(), count), (3, 5));
        assert!(found[0].norm() < 1e-3, "nearest first: {found:?}");
    }

    #[test]
    fn an_empty_domain_has_no_sources() {
        assert!(sources("z", domain(1.0, -1.0, -1.0, 1.0), Complex::new(0.0, 0.0)).is_empty());
    }

    #[test]
    fn decimals_follow_the_zoom() {
        // Points 1/150 apart need 3 decimals; 1/1e6 apart, 6.
        assert_eq!(decimals(150.0, 0.5), 3);
        assert_eq!(decimals(100.0, 0.5), 2);
        assert_eq!(decimals(1e6, 0.5), 6);
        assert_eq!(decimals(1.0, 0.5), 0);
        // An f32 near 50 has 5 decimals to give, near 2e7 none.
        assert_eq!(decimals(1e6, 50.0), 5);
        assert_eq!(decimals(1e6, 2e7), 0);
    }

    #[test]
    fn complex_numbers_read_x_plus_yi() {
        assert_eq!(complex(Complex::new(0.5, 1.25), 2), "0.50 + 1.25i");
        assert_eq!(complex(Complex::new(-0.5, -1.25), 2), "−0.50 − 1.25i");
        assert_eq!(
            complex(Complex::new(1.0000019, 0.0), 6),
            "1.000002 + 0.000000i"
        );
        // No negative zero.
        assert_eq!(complex(Complex::new(-0.0001, -0.0001), 2), "0.00 + 0.00i");
        assert_eq!(complex(Complex::new(3.0, -2.0), 0), "3 − 2i");
    }

    #[test]
    fn stretch_and_turn() {
        assert_eq!(significant(2.3141), "2.31");
        assert_eq!(significant(0.43149), "0.431");
        assert_eq!(significant(231.4), "231");
        assert_eq!(significant(2314.0), "2314");
        assert_eq!(degrees(-145.2f32.to_radians()), "−145°");
        assert_eq!(degrees(-0.2f32.to_radians()), "0°");
    }
}
