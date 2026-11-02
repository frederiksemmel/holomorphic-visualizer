use num::complex::Complex;

/// Highest grid refinement level; `2^MAX_RESOLUTION` lines per unit.
pub const MAX_RESOLUTION: f32 = 7.0;

/// Generates the points of a square grid covering `[x_min, x_max] x [y_min, y_max]`.
///
/// There are `2^resolution` grid lines per unit length, and each grid segment is
/// subdivided into `segment_resolution` points so the lines stay smooth after a
/// non-linear map is applied.
///
/// Returns a flat iterator over all points plus the length of each polyline:
/// consume `line_structure[0]` points for the first line, then
/// `line_structure[1]` for the next, and so on.
pub fn create_gridlines(
    segment_resolution: f32,
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
) -> (impl Iterator<Item = Complex<f32>>, Vec<usize>) {
    let lines_per_unit = 2.0f32.powi(resolution.floor().clamp(0.0, MAX_RESOLUTION) as i32);
    let resolution = lines_per_unit * segment_resolution;

    let snap_up = |v: f32| (v * lines_per_unit).ceil() / lines_per_unit;
    let snap_down = |v: f32| (v * lines_per_unit).floor() / lines_per_unit;
    let x_coarse_min = snap_up(x_min);
    let x_coarse_max = snap_down(x_max);
    let y_coarse_min = snap_up(y_min);
    let y_coarse_max = snap_down(y_max);

    let count = |min: f32, max: f32, per_unit: f32| {
        if max < min {
            0
        } else {
            ((max - min) * per_unit).round() as usize + 1
        }
    };
    let x_len = count(x_coarse_min, x_coarse_max, resolution);
    let x_coarse_len = count(x_coarse_min, x_coarse_max, lines_per_unit);
    let y_len = count(y_coarse_min, y_coarse_max, resolution);
    let y_coarse_len = count(y_coarse_min, y_coarse_max, lines_per_unit);

    let xs = (0..x_len).map(move |i| i as f32 / resolution + x_coarse_min);
    let ys_coarse = (0..y_coarse_len).map(move |i| i as f32 / lines_per_unit + y_coarse_min);
    let x_lines = ys_coarse.flat_map(move |y| xs.clone().map(move |x| Complex::new(x, y)));

    let ys = (0..y_len).map(move |i| i as f32 / resolution + y_coarse_min);
    let xs_coarse = (0..x_coarse_len).map(move |i| i as f32 / lines_per_unit + x_coarse_min);
    let y_lines = xs_coarse.flat_map(move |x| ys.clone().map(move |y| Complex::new(x, y)));

    let points = x_lines.chain(y_lines);
    let mut line_structure = vec![x_len; y_coarse_len];
    line_structure.extend(vec![y_len; x_coarse_len]);

    (points, line_structure)
}

/// Naive reference implementation, kept for benchmarking against [`create_gridlines`].
pub fn create_gridlines_simple(
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
) -> Vec<Vec<(f32, f32)>> {
    let x_len = ((x_max.floor() - x_min.ceil() + 1.0) * resolution) as usize;
    let xs: Vec<f32> = (0..x_len)
        .map(|x| x as f32 / resolution + x_min.ceil())
        .collect();
    let y_len = ((y_max.floor() - y_min.ceil() + 1.0) * resolution) as usize;
    let ys: Vec<f32> = (0..y_len)
        .map(|y| y as f32 / resolution + y_min.ceil())
        .collect();

    let x_lines = ys.iter().map(|&y| xs.iter().map(|&x| (x, y)).collect());
    let y_lines = xs.iter().map(|&x| ys.iter().map(|&y| (x, y)).collect());
    x_lines.chain(y_lines).collect()
}

/// The grid as the GPU draws it: every grid line is cut into straight segments
/// and each segment is one instance of a quad.
///
/// Covers the same lines as [`create_gridlines`]. `segment(i)` must match the
/// `segment` function in `src/grid.wgsl`.
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
    /// per unit length but no more than `max_segments` in total.
    pub fn new(
        resolution: f32,
        x_min: f32,
        x_max: f32,
        y_min: f32,
        y_max: f32,
        max_density: f32,
        max_segments: u32,
    ) -> Self {
        let lines_per_unit = 2.0f32.powi(resolution.floor().clamp(0.0, MAX_RESOLUTION) as i32);
        let snap_up = |v: f32| (v * lines_per_unit).ceil() / lines_per_unit;
        let snap_down = |v: f32| (v * lines_per_unit).floor() / lines_per_unit;
        let min = Complex::new(snap_up(x_min), snap_up(y_min));
        let max = Complex::new(snap_down(x_max), snap_down(y_max));

        let lines = |min: f32, max: f32| {
            if max < min {
                0
            } else {
                ((max - min) * lines_per_unit).round() as u32 + 1
            }
        };
        let (h_lines, v_lines) = (lines(min.im, max.im), lines(min.re, max.re));
        let (width, height) = ((max.re - min.re).max(0.0), (max.im - min.im).max(0.0));

        let total_length = h_lines as f32 * width + v_lines as f32 * height;
        let density = max_density.min(max_segments as f32 / total_length.max(f32::EPSILON));
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
            spacing: 1.0 / lines_per_unit,
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
    fn layout_covers_the_same_lines_as_create_gridlines() {
        let (x_min, x_max, y_min, y_max) = (-1.3, 2.7, -0.5, 1.0);
        for resolution in [0.0, 1.5, 3.0] {
            let layout = GridLayout::new(resolution, x_min, x_max, y_min, y_max, 64.0, u32::MAX);
            let (points, lines) = create_gridlines(4.0, resolution, x_min, x_max, y_min, y_max);
            let points: Vec<_> = points.collect();

            assert_eq!((layout.h_lines + layout.v_lines) as usize, lines.len());
            let n = layout.segment_count();
            assert_eq!(layout.segment(0).0, points[0]);
            assert!((layout.segment(n - 1).1 - points[points.len() - 1]).norm() < 1e-5);
        }
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
    fn line_structure_matches_point_count() {
        for resolution in [0.0, 1.5, 3.0, 7.0, 12.0] {
            let (points, lines) = create_gridlines(4.0, resolution, -1.3, 2.7, -0.5, 1.0);
            assert_eq!(points.count(), lines.iter().sum::<usize>());
        }
    }

    #[test]
    fn unit_square_at_resolution_zero() {
        let (points, lines) = create_gridlines(4.0, 0.0, 0.0, 1.0, 0.0, 1.0);
        // Two horizontal and two vertical lines, each with 4 segments => 5 points.
        assert_eq!(lines, vec![5, 5, 5, 5]);
        let points: Vec<_> = points.collect();
        assert_eq!(points.first(), Some(&Complex::new(0.0, 0.0)));
        assert_eq!(points[4], Complex::new(1.0, 0.0));
        assert_eq!(points.last(), Some(&Complex::new(1.0, 1.0)));
    }

    #[test]
    fn empty_range_yields_no_lines() {
        let (points, lines) = create_gridlines(4.0, 0.0, 0.2, 0.8, 0.0, 1.0);
        assert_eq!(points.count(), 0);
        assert!(lines.iter().all(|&l| l == 0));
    }
}
