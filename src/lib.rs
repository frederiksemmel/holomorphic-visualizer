use nannou::prelude::*;
use num::complex::Complex;

pub fn create_gridlines(
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
) -> (impl Iterator<Item = Complex<f32>>, Vec<usize>) {
    let points_per_grid = 8f32;
    let resolution = pow(2.0f32, resolution.floor().min(7.0) as usize) * points_per_grid;
    let x_min = (x_min * resolution).ceil() / resolution;
    let x_coarse_min = (x_min * (resolution / points_per_grid) as f32).ceil()
        / (resolution / points_per_grid) as f32;
    let x_max = (x_max * resolution).floor() / resolution;
    let x_coarse_max = (x_max * (resolution / points_per_grid) as f32).floor()
        / (resolution / points_per_grid) as f32;
    let y_min = (y_min * resolution).ceil() / resolution;
    let y_coarse_min = (y_min * (resolution / points_per_grid) as f32).ceil()
        / (resolution / points_per_grid) as f32;
    let y_max = (y_max * resolution).floor() / resolution;
    let y_coarse_max = (y_max * (resolution / points_per_grid) as f32).floor()
        / (resolution / points_per_grid) as f32;

    let x_len = ((x_max - x_min) * resolution) as usize +1;
    let x_coarse_len = ((x_coarse_max - x_coarse_min) * (resolution / points_per_grid)) as usize +1;
    let y_len = ((y_max - y_min) * resolution) as usize +1;
    let y_coarse_len = ((y_coarse_max - y_coarse_min) * (resolution / points_per_grid)) as usize +1;

    let xs = (0i16..x_len as i16).map(move |x| x as f32 / resolution + x_min);
    let ys_coarse = (0i16..y_coarse_len as i16)
        .map(move |x| x as f32 / (resolution / points_per_grid) + y_coarse_min);
    let x_lines = ys_coarse.flat_map(move |y| xs.clone().map(move |x| Complex::new(x, y)));

    let ys = (0i16..y_len as i16).map(move |y| y as f32 / resolution + y_min);
    let xs_coarse = (0i16..x_coarse_len as i16)
        .map(move |x| x as f32 / (resolution / points_per_grid) + x_coarse_min);
    let y_lines = xs_coarse.flat_map(move |x| ys.clone().map(move |y| Complex::new(x, y)));

    let points = x_lines.chain(y_lines);
    let mut line_structure = vec![x_len; y_coarse_len];
    line_structure.extend(vec![y_len; x_coarse_len]);

    (points, line_structure)
}
pub fn create_gridlines_simple(
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
) -> Vec<Vec<(f32, f32)>> {
    let x_len: i16 = ((x_max.floor() - x_min.ceil() + 1.0) * resolution) as i16;
    let xs: Vec<f32> = (0i16..x_len)
        .map(std::convert::From::from)
        .map(|x: f32| x / resolution + x_min.ceil())
        .collect();
    let y_len: i16 = ((y_max.floor() - y_min.ceil() + 1.0) * resolution) as i16;
    let ys: Vec<f32> = (0i16..y_len)
        .map(std::convert::From::from)
        .map(|y: f32| y / resolution + y_min.ceil())
        .collect();

    let mut x_lines: Vec<Vec<(f32, f32)>> = Vec::new();
    for y in &ys {
        let mut x_line: Vec<(f32, f32)> = Vec::new();
        for x in &xs {
            x_line.push((*x, *y));
        }
        x_lines.push(x_line);
    }

    let mut y_lines: Vec<Vec<(f32, f32)>> = Vec::new();
    for x in &xs {
        let mut y_line: Vec<(f32, f32)> = Vec::new();
        for y in &ys {
            y_line.push((*x, *y));
        }
        y_lines.push(y_line);
    }
    [&x_lines[..], &y_lines[..]].concat()
}
