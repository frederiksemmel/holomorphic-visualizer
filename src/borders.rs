//! Taking hold of a border of the grid on screen, wherever the map has put it.
//!
//! The grid is drawn on the GPU; these work with a CPU copy of the map (see
//! [`crate::expr::Function::eval`]), passed in as `to_screen`: a source point
//! to where it is drawn, in screen points. Screen points are complex numbers
//! here too, with y up.

use num::Complex;

/// Points sampled along each border when looking for the one under the cursor.
const SAMPLES: usize = 256;
/// Rounds of ternary search that refine the best sample.
const REFINE_ROUNDS: usize = 24;
/// Largest move of a border per drag step or scroll event, in grid units.
/// Near a critical point of the map (h' ≈ 0) a small cursor motion asks for
/// an arbitrarily large move of the source border; capping it makes the
/// border lag behind the cursor there instead of leaping across the plane.
pub const MAX_STEP: f64 = 0.25;

/// One of the four sides of the grid domain `[x_min, x_max, y_min, y_max]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Border {
    XMin,
    XMax,
    YMin,
    YMax,
}

impl Border {
    pub const ALL: [Border; 4] = [Border::XMin, Border::XMax, Border::YMin, Border::YMax];

    /// Where this border's value is in `[x_min, x_max, y_min, y_max]`.
    pub fn index(self) -> usize {
        self as usize
    }

    /// x_min and x_max are lines of constant x.
    fn vertical(self) -> bool {
        matches!(self, Border::XMin | Border::XMax)
    }

    /// The point a fraction `s` of the way along this border of `grid`.
    pub fn point(self, grid: [f64; 4], s: f64) -> Complex<f64> {
        let [x_min, x_max, y_min, y_max] = grid;
        let lerp = |a: f64, b: f64| a + (b - a) * s;
        match self {
            Border::XMin => Complex::new(x_min, lerp(y_min, y_max)),
            Border::XMax => Complex::new(x_max, lerp(y_min, y_max)),
            Border::YMin => Complex::new(lerp(x_min, x_max), y_min),
            Border::YMax => Complex::new(lerp(x_min, x_max), y_max),
        }
    }

    /// How far to move this border so that, where the map has derivative
    /// `derivative`, its image follows the cursor moving by `delta` plane
    /// units. Moving a line of constant x by δ moves its image by h'·δ, and
    /// the image runs along h'·i, so only the part of delta / h' across the
    /// line counts: its real part for x, its imaginary part for y.
    pub fn drag_step(self, derivative: Complex<f64>, delta: Complex<f64>) -> f64 {
        let across = delta / derivative;
        capped(if self.vertical() {
            across.re
        } else {
            across.im
        })
    }

    /// How much of a scroll of `delta` (screen points, y up) goes across this
    /// border, where the map has derivative `derivative`: positive the way
    /// that grows the border's coordinate. Only the direction of the border
    /// on screen matters, not how much the map stretches it, and a scroll
    /// along the border does nothing. A mouse `wheel` only scrolls one way,
    /// often along the border, so for a wheel a scroll that goes more along
    /// it than across counts as across: up or right moves it up or right.
    /// Zero where the map blows up.
    pub fn scroll_across(self, derivative: Complex<f64>, delta: Complex<f64>, wheel: bool) -> f64 {
        // The image of the border runs along h'·i for x, h' for y; it is
        // conformal, so its normal on screen runs along h' for x, h'·i for y.
        let normal = if self.vertical() {
            derivative
        } else {
            derivative * Complex::i()
        };
        let normal = normal / normal.norm();
        let local = delta * normal.conj();
        let across = if !wheel || local.re.abs() >= local.im.abs() {
            local.re
        } else {
            (delta.re + delta.im).signum() * (normal.re + normal.im).signum() * delta.norm()
        };
        if across.is_finite() { across } else { 0.0 }
    }
}

/// `step`, at most [`MAX_STEP`] either way; nothing where the map blows up.
fn capped(step: f64) -> f64 {
    if step.is_finite() {
        step.clamp(-MAX_STEP, MAX_STEP)
    } else {
        0.0
    }
}

/// Where on `border` the drawn grid comes closest to `target`: the source
/// point, and its distance on screen. `None` if the map is not finite
/// anywhere on the border.
pub fn nearest_on(
    border: Border,
    grid: [f64; 4],
    to_screen: &impl Fn(Complex<f64>) -> Complex<f64>,
    target: Complex<f64>,
) -> Option<(Complex<f64>, f64)> {
    let distance = |s: f64| {
        let d = (to_screen(border.point(grid, s)) - target).norm_sqr();
        if d.is_finite() { d } else { f64::INFINITY }
    };
    let step = 1.0 / SAMPLES as f64;
    let (best, best_distance) = (0..=SAMPLES)
        .map(|i| (i, distance(i as f64 * step)))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    if best_distance == f64::INFINITY {
        return None;
    }
    // The image between the neighbouring samples is close to a line, so the
    // distance has a single minimum there.
    let (mut lo, mut hi) = (
        (best as f64 - 1.0).max(0.0) * step,
        (best as f64 + 1.0).min(SAMPLES as f64) * step,
    );
    for _ in 0..REFINE_ROUNDS {
        let (a, b) = (lo + (hi - lo) / 3.0, hi - (hi - lo) / 3.0);
        if distance(a) <= distance(b) {
            hi = b;
        } else {
            lo = a;
        }
    }
    let s = 0.5 * (lo + hi);
    let (s, d) = [(s, distance(s)), (best as f64 * step, best_distance)]
        .into_iter()
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    Some((border.point(grid, s), d.sqrt()))
}

/// The border of `grid` drawn closest to `target`, and the source point on
/// it that lands there.
pub fn nearest_border(
    grid: [f64; 4],
    to_screen: &impl Fn(Complex<f64>) -> Complex<f64>,
    target: Complex<f64>,
) -> Option<(Border, Complex<f64>)> {
    Border::ALL
        .into_iter()
        .filter_map(|border| {
            let (z, d) = nearest_on(border, grid, to_screen, target)?;
            Some((border, z, d))
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
        .map(|(border, z, _)| (border, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr;

    const PPU: f64 = 100.0;
    const GRID: [f64; 4] = [-1.0, 1.0, -1.0, 1.0];

    /// Draws the grid mapped by `formula` (fully blended) at PPU, centred.
    fn screen(formula: &str) -> impl Fn(Complex<f64>) -> Complex<f64> {
        let f = expr::parse(formula).unwrap();
        move |z| f.eval(z, Complex::new(1.0, 0.0)) * PPU
    }

    #[test]
    fn nearest_border_unmapped() {
        let to_screen = screen("z");
        let (border, z) = nearest_border(GRID, &to_screen, Complex::new(95.0, 20.0)).unwrap();
        assert_eq!(border, Border::XMax);
        assert!((z - Complex::new(1.0, 0.2)).norm() < 1e-4, "{z}");
        let (border, z) = nearest_border(GRID, &to_screen, Complex::new(-30.0, -140.0)).unwrap();
        assert_eq!(border, Border::YMin);
        assert!((z - Complex::new(-0.3, -1.0)).norm() < 1e-4, "{z}");
    }

    #[test]
    fn nearest_border_follows_the_map() {
        // A quarter turn puts x = 1 at the top and y = 1 on the left.
        let to_screen = screen("i z");
        let (border, z) = nearest_border(GRID, &to_screen, Complex::new(40.0, 103.0)).unwrap();
        assert_eq!(border, Border::XMax);
        assert!((z - Complex::new(1.0, -0.4)).norm() < 1e-4, "{z}");
        let (border, _) = nearest_border(GRID, &to_screen, Complex::new(-98.0, 0.0)).unwrap();
        assert_eq!(border, Border::YMax);
    }

    #[test]
    fn a_stage_at_its_start_draws_the_stage_before() {
        // At t = 0, stage 2 of i(z + 2) draws z + 2: the grid shifted right, not at z.
        let chain = expr::parse("i * (z + 2)").unwrap().chain();
        let ends = expr::Homotopy::between(&chain, 1, 2);
        let p = Complex::new(1.0, 0.0);
        let to_screen = |z| ends.map(z, p, 0.0) * PPU;
        let (border, z) = nearest_border(GRID, &to_screen, Complex::new(310.0, 50.0)).unwrap();
        assert_eq!(border, Border::XMax);
        assert!((z - Complex::new(1.0, 0.5)).norm() < 1e-4, "{z}");
    }

    #[test]
    fn non_finite_points_are_skipped() {
        // 1/z blows up at 0, which lies on y = 0 of this grid.
        let grid = [-1.0, 1.0, 0.0, 1.0];
        let to_screen = screen("1/z");
        let (_, d) = nearest_on(Border::YMin, grid, &to_screen, Complex::new(150.0, 0.0)).unwrap();
        assert!(d < 1.0, "{d}");
    }

    /// Drags the border nearest `start` by `delta` screen points and checks
    /// that its new image passes close to where the cursor went.
    fn check_drag(formula: &str, grid: [f64; 4], start: Complex<f64>, delta: Complex<f64>) {
        let f = expr::parse(formula).unwrap();
        let p = Complex::new(1.0, 0.0);
        let to_screen = screen(formula);
        let (border, z) = nearest_border(grid, &to_screen, start).unwrap();
        let (_, d) = nearest_on(border, grid, &to_screen, start).unwrap();
        assert!(
            d < 0.01,
            "{formula}: start should be on a border, is {d} away"
        );

        let mut moved = grid;
        moved[border.index()] += border.drag_step(f.derivative(z, p), delta / PPU);
        let (_, miss) = nearest_on(border, moved, &to_screen, start + delta).unwrap();
        assert!(
            miss < 0.2,
            "{formula}: border passes {miss} points from the cursor"
        );
    }

    #[test]
    fn dragging_keeps_the_border_under_the_cursor() {
        // x = 1, y = 0.5 lands at (200, 100).
        check_drag(
            "2z",
            GRID,
            Complex::new(200.0, 100.0),
            Complex::new(10.0, 5.0),
        );
        check_drag(
            "2z",
            GRID,
            Complex::new(60.0, -200.0),
            Complex::new(-3.0, 7.0),
        );
        let grid = [-1.0, 0.5, -1.0, 1.0];
        let start = Complex::new(0.5, 0.3).exp() * PPU;
        check_drag("exp(z)", grid, start, Complex::new(3.0, -2.0));
        let start = Complex::new(-0.2, 1.0).exp() * PPU;
        check_drag("exp(z)", grid, start, Complex::new(-2.0, 4.0));
    }

    #[test]
    fn scrolling_counts_the_part_across_the_border() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-5;
        let one = Complex::new(1.0, 0.0);
        assert!(close(Border::XMax.scroll_across(one, Complex::new(10.0, 2.0), false), 10.0));
        assert!(close(Border::YMin.scroll_across(one, Complex::new(2.0, -10.0), false), -10.0));
        // However much the map stretches it.
        assert!(close(Border::XMin.scroll_across(2.0 * one, Complex::new(-10.0, 1.0), false), -10.0));
        // Turned by i, x = c is drawn as a horizontal line: up moves it across.
        let turn = Complex::i();
        assert!(close(Border::XMax.scroll_across(turn, Complex::new(1.0, 10.0), false), 10.0));
        // Along the border it stays put.
        assert!(close(Border::XMax.scroll_across(one, Complex::new(0.0, 10.0), false), 0.0));
    }

    #[test]
    fn a_wheel_along_the_border_moves_it_up_or_right() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-5;
        let one = Complex::new(1.0, 0.0);
        // A wheel scrolling vertically, along x = c: up moves it right.
        assert!(close(Border::XMin.scroll_across(one, Complex::new(0.0, 10.0), true), 10.0));
        assert!(close(Border::XMax.scroll_across(one, Complex::new(0.0, -10.0), true), -10.0));
        // Sideways, along y = c: right moves it up.
        assert!(close(Border::YMax.scroll_across(one, Complex::new(10.0, 0.0), true), 10.0));
        let nan = Complex::new(f64::NAN, 0.0);
        assert_eq!(Border::XMax.scroll_across(nan, Complex::new(1.0, 0.0), true), 0.0);
    }

    #[test]
    fn steps_are_capped_near_critical_points() {
        let tiny = Complex::new(1e-9, 0.0);
        assert_eq!(
            Border::YMin.drag_step(tiny, Complex::new(0.0, -1.0)),
            -MAX_STEP
        );
        assert_eq!(
            Border::XMax.drag_step(Complex::new(0.0, 0.0), Complex::new(1.0, 0.0)),
            0.0
        );
    }
}
