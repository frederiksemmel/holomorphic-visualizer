//! Drawing the grid far zoomed in, where f32 runs out.
//!
//! The GPU only has f32, so a point on screen computed as `(f(z) + offset) · ppu`
//! is off by |f(z)| · 1e-7 · ppu pixels, which is whole pixels from a zoom of
//! about 1e7. A game has the same problem far from its origin and solves it by
//! sending the GPU only what is near the camera. Here that takes one more step,
//! as the same trick that deep fractal zooms use: f is not a rigid motion, but
//! this far in it hardly bends across the window. So the CPU works out, in f64,
//! the cubic that follows f around one point `source` that lands in view, and
//! the GPU evaluates only that, on small numbers.
//!
//! Positions are measured in units `u` of about one screen point from `source`,
//! which keeps every number the shader sees near 1 however far in the zoom is.

use crate::GridLayout;
use crate::expr::Jet;
use num::Complex;

/// Segments of a line across the window. It is nearly straight here (see
/// `MAX_BEND`), so this is plenty.
const SEGMENTS: u32 = 32;
/// How far past the window, as a share of its radius, lines are drawn: the
/// patch bends a little.
const MARGIN: f64 = 1.25;
/// How much the map may bend across the window for its cubic to be trusted:
/// the term after the cubic is then about `reach · MAX_BEND³`, under a tenth of
/// a point for any window size.
const MAX_BEND: f64 = 0.03;
/// Stands in for a spacing no f32 can hold. With at most one line on an axis,
/// the spacing is never used, but `0 · infinity` would still reach the shader.
const MAX_SPACING: f64 = 1e30;

/// The cubic around `source`, and the grid lines in view, for the GPU.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Patch {
    /// The grid point the cubic is centred on.
    pub source: Complex<f64>,
    /// Source plane units per unit of u.
    pub units_per_u: f64,
    /// On screen, from the window centre: a0 + a1·u + a2·u² + a3·u³.
    pub cubic: [Complex<f64>; 4],
    /// The lines in view, in u. Lines run from `min` to `max`, so as not to
    /// reach out to the next line off screen, where the cubic is no good.
    pub layout: GridLayout,
    /// Where the first horizontal and vertical line of `layout` are, in u.
    pub lines_from: Complex<f64>,
}

impl Patch {
    /// The patch around `source`, where the map has the Taylor series `jet`,
    /// for a view at `position` and `pixels_per_unit` that reaches `reach`
    /// points from its centre, showing the lines of `grid`. `None` if no cubic
    /// will do: the map bends too much across the window, `source` is off
    /// screen, or too many lines are in view for `max_segments`.
    pub fn new(
        source: Complex<f64>,
        jet: Jet,
        position: Complex<f64>,
        pixels_per_unit: f64,
        reach: f64,
        grid: &GridLayout,
        max_segments: u32,
    ) -> Option<Patch> {
        let ppu = pixels_per_unit;
        let units_per_u = 1.0 / (ppu * jet[1].norm());
        // In this order: `units_per_u²` can underflow far zoomed in.
        let cubic = [
            (jet[0] + position) * ppu,
            jet[1] * (units_per_u * ppu),
            jet[2] * (units_per_u * ppu) * units_per_u,
            jet[3] * (units_per_u * ppu) * units_per_u * units_per_u,
        ];
        let usable = units_per_u.is_finite()
            && units_per_u > 0.0
            && cubic.iter().all(|a| a.re.is_finite() && a.im.is_finite());
        let bend = (cubic[2].norm() * reach).max(cubic[3].norm().sqrt() * reach);
        if !usable || bend > MAX_BEND || cubic[0].norm() > reach {
            return None;
        }

        // Lines within this many u of `source` can land in the window.
        let half = (MARGIN * reach + cubic[0].norm()) * units_per_u;
        let to_u = |x: f64, centre: f64| (x - centre) / units_per_u;
        // The lines on one axis, numbered from the grid's first, and how far
        // they run along the other axis.
        let axis = |min: f64, max: f64, lines: u32, centre: f64| {
            let (low, high) = (centre - half, centre + half);
            let first = ((low - min) / grid.spacing).ceil().max(0.0);
            let last = ((high - min) / grid.spacing).floor().min(lines as f64 - 1.0);
            let count = (last - first + 1.0).max(0.0);
            let from = to_u(min + first * grid.spacing, centre);
            (count, from, to_u(low.max(min), centre), to_u(high.min(max), centre))
        };
        let (v_lines, x_from, x_min, x_max) =
            axis(grid.min.re, grid.max.re, grid.v_lines, source.re);
        let (h_lines, y_from, y_min, y_max) =
            axis(grid.min.im, grid.max.im, grid.h_lines, source.im);
        let (h_lines, v_lines) = if x_min <= x_max && y_min <= y_max {
            (h_lines, v_lines)
        } else {
            (0.0, 0.0)
        };
        if (h_lines + v_lines) * SEGMENTS as f64 > max_segments as f64 {
            return None;
        }
        let layout = GridLayout {
            min: Complex::new(x_min, y_min),
            max: Complex::new(x_max, y_max),
            spacing: (grid.spacing / units_per_u).min(MAX_SPACING),
            h_lines: h_lines as u32,
            v_lines: v_lines as u32,
            h_segments: SEGMENTS,
            v_segments: SEGMENTS,
        };
        let lines_from = Complex::new(x_from, y_from);
        Some(Patch { source, units_per_u, cubic, layout, lines_from })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{self, Homotopy};

    const P: Complex<f64> = Complex::new(0.5, 0.25);
    const REACH: f64 = 500.0;
    const BUDGET: u32 = 1_000_000;

    fn grid() -> GridLayout {
        GridLayout::new(2.0, -4.0, 4.0, -4.0, 4.0, 2048.0, BUDGET)
    }

    fn at(cubic: &[Complex<f64>; 4], u: Complex<f64>) -> Complex<f64> {
        cubic[0] + u * (cubic[1] + u * (cubic[2] + u * cubic[3]))
    }

    /// The patch of `formula` around `source`, with `source` at the window centre.
    fn patch(formula: &str, source: Complex<f64>, ppu: f64) -> Option<Patch> {
        let f = expr::parse(formula).unwrap();
        let position = -f.eval(source, P);
        Patch::new(source, f.jet(source, P), position, ppu, REACH, &grid(), BUDGET)
    }

    /// The cubic must land where the map does, here in f64, to a fraction of a
    /// point across the whole window.
    #[test]
    fn the_cubic_lands_where_the_map_does() {
        let source = Complex::new(0.7, 0.4);
        let formulas = ["exp(p*z)", "sin(cos(p*z))", "z^3", "1/z", "z^p", "sqrt(z)", "tan(z)", "z + p/z"];
        for formula in formulas {
            for ppu in [1e6, 1e8, 1e10, 1e13] {
                let f = expr::parse(formula).unwrap();
                let position = -f.eval(source, P);
                let patch = patch(formula, source, ppu).unwrap_or_else(|| panic!("{formula}"));
                for (x, y) in [(0.0, 0.0), (REACH, 0.0), (-300.0, 300.0), (120.0, -450.0)] {
                    let u = Complex::new(x, y);
                    let z = source + u * patch.units_per_u;
                    let exact = (f.eval(z, P) + position) * ppu;
                    let miss = (at(&patch.cubic, u) - exact).norm();
                    assert!(miss < 0.1, "{formula} at {ppu}, {u}: {miss} points off");
                }
            }
        }
    }

    #[test]
    fn numbers_stay_small_for_the_gpu_at_any_zoom() {
        let source = Complex::new(0.7, 0.4);
        for ppu in [1e6, 1e12, 1e15] {
            let patch = patch("exp(p*z)", source, ppu).unwrap();
            assert!((patch.cubic[1].norm() - 1.0).abs() < 1e-9, "{:?}", patch.cubic);
            assert!(patch.layout.max.re.abs() < 1e4, "{:?}", patch.layout);
            assert!(patch.cubic.iter().all(|a| a.norm() < 2.0), "{:?}", patch.cubic);
        }
    }

    #[test]
    fn a_map_that_bends_across_the_window_is_refused() {
        // Zoomed out, and at the critical point of z², where nothing is linear.
        assert_eq!(patch("exp(z)", Complex::new(0.5, 0.5), 100.0), None);
        assert_eq!(patch("z^2", Complex::new(0.0, 0.0), 1e9), None);
    }

    #[test]
    fn a_source_off_screen_is_refused() {
        let f = expr::parse("z^2").unwrap();
        let source = Complex::new(1.0, 0.0);
        let position = Complex::new(-1.1, 0.0);
        let refused = Patch::new(source, f.jet(source, P), position, 1e9, REACH, &grid(), BUDGET);
        assert_eq!(refused, None);
    }

    #[test]
    fn only_the_lines_in_view_are_laid_out() {
        // Lines every 0.25. 1 lies on one in both directions, the view is around it.
        let ppu = 1e9;
        let patch = patch("z", Complex::new(1.0, 1.0), ppu).unwrap();
        assert_eq!((patch.layout.h_lines, patch.layout.v_lines), (1, 1));
        assert!(patch.lines_from.norm() < 1e-3, "{}", patch.lines_from);
        // They run across the view, not out to the next line 2.5e8 points away.
        assert!(patch.layout.max.re < 2.0 * REACH);

        // Between lines there is nothing to draw.
        let empty = self::patch("z", Complex::new(1.1, 1.1), ppu).unwrap();
        assert_eq!((empty.layout.h_lines, empty.layout.v_lines), (0, 0));
        assert_eq!(empty.layout.segment_count(), 0);
    }

    #[test]
    fn nothing_is_drawn_outside_the_grid() {
        let outside = patch("z", Complex::new(9.0, 1.0), 1e9).unwrap();
        assert_eq!(outside.layout.segment_count(), 0);
    }

    #[test]
    fn too_many_lines_in_view_are_refused() {
        // z/1e9 draws the grid so small that all of it is in view.
        let source = Complex::new(0.1, 0.1);
        let f = expr::parse("z/1000000000").unwrap();
        let position = -f.eval(source, P);
        let patch = Patch::new(source, f.jet(source, P), position, 1e9, REACH, &grid(), 16);
        assert_eq!(patch, None);
    }

    #[test]
    fn a_stage_blends_its_ends() {
        let stages = expr::parse("sin(cos(p*z))").unwrap().stages();
        let ends = Homotopy::of_stage(&stages, 2).unwrap();
        let (source, t, ppu) = (Complex::new(0.7, 0.4), 0.3, 1e9);
        let position = -ends.map(source, P, t);
        let patch = Patch::new(source, ends.jet(source, P, t), position, ppu, REACH, &grid(), BUDGET)
            .unwrap();
        let u = Complex::new(200.0, -350.0);
        let z = source + u * patch.units_per_u;
        let exact = (ends.map(z, P, t) + position) * ppu;
        assert!((at(&patch.cubic, u) - exact).norm() < 0.1);
    }
}
