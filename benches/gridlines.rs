use std::hint::black_box;

use conformal_visualizer::{create_gridlines, create_gridlines_simple};
use criterion::{Criterion, criterion_group, criterion_main};
use num::complex::Complex;

fn consume_and_map(points: impl Iterator<Item = Complex<f32>>) {
    let points = points.map(|x| x * x).collect::<Vec<Complex<f32>>>();
    black_box(points);
}

pub fn criterion_benchmark(c: &mut Criterion) {
    for resolution in [0.0, 4.0] {
        c.bench_function(
            &format!("gridlines iterator, resolution {resolution}"),
            |b| {
                b.iter(|| {
                    let (points, _line_structure) = create_gridlines(
                        black_box(4.0),
                        black_box(resolution),
                        black_box(-5.0),
                        black_box(5.0),
                        black_box(-5.0),
                        black_box(5.0),
                    );
                    consume_and_map(points);
                })
            },
        );
    }

    for resolution in [1.0, 16.0] {
        c.bench_function(&format!("gridlines vector, resolution {resolution}"), |b| {
            b.iter(|| {
                create_gridlines_simple(
                    black_box(resolution),
                    black_box(-5.0),
                    black_box(5.0),
                    black_box(-5.0),
                    black_box(5.0),
                )
            })
        });
    }
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
