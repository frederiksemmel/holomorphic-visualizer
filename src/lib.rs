use num::complex::Complex;

/// Highest grid refinement level; `2^MAX_RESOLUTION` lines per unit.
pub const MAX_RESOLUTION: f32 = 7.0;

/// The refinement level `resolution` asks for: `2^level` lines per unit.
fn level(resolution: f32) -> i32 {
    resolution.floor().clamp(0.0, MAX_RESOLUTION) as i32
}

/// The part of the grid domain `[min, max]` that a view of `[view_min,
/// view_max]` needs: its lines inside the view, plus the next line out on each
/// side, so lines that cross the view still reach its edges.
///
/// Only valid while the grid is drawn unmapped; under a map, lines from
/// anywhere in the domain can land in the view.
pub fn clip_to_view(
    resolution: f32,
    min: Complex<f32>,
    max: Complex<f32>,
    view_min: Complex<f32>,
    view_max: Complex<f32>,
) -> (Complex<f32>, Complex<f32>) {
    let lines_per_unit = 2.0f32.powi(level(resolution));
    let outward_down = |v: f32| (v * lines_per_unit).floor() / lines_per_unit;
    let outward_up = |v: f32| (v * lines_per_unit).ceil() / lines_per_unit;
    (
        Complex::new(
            min.re.max(outward_down(view_min.re)),
            min.im.max(outward_down(view_min.im)),
        ),
        Complex::new(
            max.re.min(outward_up(view_max.re)),
            max.im.min(outward_up(view_max.im)),
        ),
    )
}

/// The grid as the GPU draws it: every grid line is cut into straight segments
/// and each segment is one instance of a quad.
///
/// `segment(i)` must match the `segment` function in `src/grid.wgsl`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridLayout {
    /// Lower-left and upper-right grid crossings.
    pub min: Complex<f32>,
    pub max: Complex<f32>,
    /// Distance between neighbouring lines.
    pub spacing: f32,
    /// Lines of constant y (horizontal) and of constant x (vertical).
    pub h_lines: u32,
    pub v_lines: u32,
    /// Segments per horizontal and per vertical line.
    pub h_segments: u32,
    pub v_segments: u32,
}

impl GridLayout {
    /// `2^resolution` lines per unit, sampled with up to `max_density` segments
    /// per unit length. There are never more than `max_segments` segments in
    /// total: the sampling gets coarser first, and if even one segment per line
    /// is too many, the lines get further apart than `resolution` asks for.
    pub fn new(
        resolution: f32,
        x_min: f32,
        x_max: f32,
        y_min: f32,
        y_max: f32,
        max_density: f32,
        max_segments: u32,
    ) -> Self {
        let lines = |min: f32, max: f32, lines_per_unit: f32| {
            if max < min {
                0
            } else {
                ((max - min) * lines_per_unit).round() as u32 + 1
            }
        };
        let mut level = level(resolution);
        let (spacing, min, max, h_lines, v_lines) = loop {
            let lines_per_unit = 2.0f32.powi(level);
            let snap_up = |v: f32| (v * lines_per_unit).ceil() / lines_per_unit;
            let snap_down = |v: f32| (v * lines_per_unit).floor() / lines_per_unit;
            let min = Complex::new(snap_up(x_min), snap_up(y_min));
            let max = Complex::new(snap_down(x_max), snap_down(y_max));
            let h_lines = lines(min.im, max.im, lines_per_unit);
            let v_lines = lines(min.re, max.re, lines_per_unit);
            // Every line takes at least one segment.
            if h_lines.saturating_add(v_lines) <= max_segments || level <= -64 {
                break (1.0 / lines_per_unit, min, max, h_lines, v_lines);
            }
            level -= 1;
        };
        let (width, height) = ((max.re - min.re).max(0.0), (max.im - min.im).max(0.0));

        // Each line rounds its share down but takes at least one segment, so it
        // can use up to one more than `length * density`. Leaving one per line
        // out of the budget keeps the total within `max_segments`.
        let total_length = h_lines as f32 * width + v_lines as f32 * height;
        let spare = max_segments.saturating_sub(h_lines.saturating_add(v_lines)) as f32;
        let density = max_density.min(spare / total_length.max(f32::EPSILON));
        let segments = |length: f32| {
            if length > 0.0 {
                ((length * density).floor() as u32).max(1)
            } else {
                0
            }
        };

        GridLayout {
            min,
            max,
            spacing,
            h_lines,
            v_lines,
            h_segments: segments(width),
            v_segments: segments(height),
        }
    }

    pub fn segment_count(&self) -> u32 {
        self.h_lines * self.h_segments + self.v_lines * self.v_segments
    }

    /// Endpoints of segment `i`, for `i < segment_count()`. Horizontal lines
    /// come first, bottom to top, then vertical lines, left to right.
    pub fn segment(&self, i: u32) -> (Complex<f32>, Complex<f32>) {
        let h_total = self.h_lines * self.h_segments;
        if i < h_total {
            let (line, k) = (i / self.h_segments, i % self.h_segments);
            let y = self.min.im + line as f32 * self.spacing;
            let step = (self.max.re - self.min.re) / self.h_segments as f32;
            let x = |k: u32| self.min.re + k as f32 * step;
            (Complex::new(x(k), y), Complex::new(x(k + 1), y))
        } else {
            let i = i - h_total;
            let (line, k) = (i / self.v_segments, i % self.v_segments);
            let x = self.min.re + line as f32 * self.spacing;
            let step = (self.max.im - self.min.im) / self.v_segments as f32;
            let y = |k: u32| self.min.im + k as f32 * step;
            (Complex::new(x, y(k)), Complex::new(x, y(k + 1)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_snaps_to_the_lines_inside_the_domain() {
        // One line per unit: x = -1, 0, 1, 2 and y = 0, 1.
        let layout = GridLayout::new(0.0, -1.3, 2.7, -0.5, 1.0, 4.0, u32::MAX);
        assert_eq!((layout.v_lines, layout.h_lines), (4, 2));
        assert_eq!((layout.min, layout.max), (Complex::new(-1.0, 0.0), Complex::new(2.0, 1.0)));

        let n = layout.segment_count();
        assert_eq!(n, 2 * 12 + 4 * 4);
        assert_eq!(layout.segment(0).0, layout.min);
        assert!((layout.segment(n - 1).1 - layout.max).norm() < 1e-5);
    }

    #[test]
    fn layout_segments_are_contiguous_along_each_line() {
        let layout = GridLayout::new(2.0, 0.0, 1.0, 0.0, 1.0, 16.0, u32::MAX);
        assert_eq!((layout.h_lines, layout.h_segments), (5, 16));
        for i in 1..layout.segment_count() {
            if i % layout.h_segments != 0 {
                assert_eq!(layout.segment(i - 1).1, layout.segment(i).0);
            }
        }
    }

    #[test]
    fn layout_respects_the_segment_budget() {
        let layout = GridLayout::new(6.0, -15.0, 15.0, -15.0, 15.0, 2048.0, 1_000_000);
        assert!(layout.segment_count() <= 1_000_000);
        assert!(layout.h_segments >= 1);
    }

    #[test]
    fn budget_holds_even_when_every_line_needs_its_own_segment() {
        // 2^7 lines per unit over 4e4 units would be ~10M lines.
        let layout = GridLayout::new(7.0, -2e4, 2e4, -2e4, 2e4, 2048.0, 1_000_000);
        assert!(layout.segment_count() <= 1_000_000, "{layout:?}");
        assert!(layout.spacing > 1.0 / 128.0, "lines should be coarser than asked");
        assert!(layout.h_segments >= 1 && layout.v_segments >= 1);
    }

    #[test]
    fn budget_holds_for_many_lines_and_small_budgets() {
        for max_segments in [10, 1_000, 99_999] {
            for resolution in [0.0, 3.0, 7.0] {
                let layout =
                    GridLayout::new(resolution, -7.3, 9.1, -2.2, 3.4, 2048.0, max_segments);
                assert!(layout.segment_count() <= max_segments, "{layout:?}");
            }
        }
    }

    fn clipped_layout(view_min: Complex<f32>, view_max: Complex<f32>) -> GridLayout {
        let (min, max) = (Complex::new(-15.0, -15.0), Complex::new(15.0, 15.0));
        let (min, max) = clip_to_view(0.0, min, max, view_min, view_max);
        GridLayout::new(0.0, min.re, max.re, min.im, max.im, 4.0, u32::MAX)
    }

    #[test]
    fn clipping_keeps_the_visible_lines_and_one_more_on_each_side() {
        let layout = clipped_layout(Complex::new(-1.5, 0.2), Complex::new(2.5, 0.7));
        // x = -2..=3 and y = 0..=1, so horizontal lines run past both view edges.
        assert_eq!((layout.min, layout.max), (Complex::new(-2.0, 0.0), Complex::new(3.0, 1.0)));
        assert_eq!((layout.v_lines, layout.h_lines), (6, 2));
    }

    #[test]
    fn clipping_never_grows_the_domain() {
        let layout = clipped_layout(Complex::new(-100.0, -100.0), Complex::new(100.0, 100.0));
        assert_eq!((layout.min, layout.max), (Complex::new(-15.0, -15.0), Complex::new(15.0, 15.0)));
    }

    #[test]
    fn a_view_beside_the_domain_draws_nothing() {
        let layout = clipped_layout(Complex::new(20.0, 0.0), Complex::new(30.0, 1.0));
        assert_eq!(layout.segment_count(), 0);
    }

    #[test]
    fn empty_range_yields_no_segments() {
        // No vertical line fits in [0.2, 0.8] at one line per unit.
        let layout = GridLayout::new(0.0, 0.2, 0.8, 0.0, 1.0, 4.0, u32::MAX);
        assert_eq!(layout.v_lines, 0);
        assert_eq!(layout.segment_count(), 0);
    }
}
