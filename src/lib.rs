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

#[cfg(test)]
mod tests {
    use super::*;

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
