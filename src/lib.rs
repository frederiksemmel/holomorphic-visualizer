use nannou::prelude::*;
use num::complex::Complex;
use cmp::max;
use mexpr::num::ComplexFloat;

pub fn create_gridlines(
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
) -> (impl Iterator<Item = Complex<f32>>, Vec<usize>)  {
    let resolution = pow(2, resolution.floor() as usize) as f32;
    let x_min = (x_min * resolution).ceil() / resolution;
    let x_max = (x_max * resolution).floor() / resolution;
    let y_min = (y_min * resolution).ceil() / resolution;
    let y_max = (y_max * resolution).floor() / resolution;
    let x_len: i16 = ((x_max - x_min) * resolution) as i16 + 1;
    let y_len: i16 = ((y_max - y_min) * resolution) as i16 + 1;
    let xs = (0i16..x_len)
        .map(std::convert::From::from)
        .map(move |x : f32| x / resolution + x_min);
    let ys = (0i16..y_len)
        .map(std::convert::From::from)
        .map(move |y : f32| y / resolution + y_min);

    let x_lines = ys
        .flat_map(move |y| xs.clone().map(move |x| Complex::new(x, y)));
    let xs = (0i16..x_len)
        .map(std::convert::From::from)
        .map(move |x : f32| x / resolution + x_min);
    let ys = (0i16..y_len)
        .map(std::convert::From::from)
        .map(move |y : f32| y / resolution + y_min);
    let y_lines = xs
        .flat_map(move |x| ys.clone().map(move |y| Complex::new(x, y)));
    let points = x_lines.chain(y_lines);
    let mut line_structure = vec![x_len as usize; cmp::max(0,y_len) as usize];
    line_structure.extend(vec![y_len as usize; cmp::max(0,x_len) as usize]);
    (points, line_structure)
}
