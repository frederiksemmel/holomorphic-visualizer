use criterion::{black_box, criterion_group, criterion_main, Criterion};
use hologrid::{create_gridlines, create_gridlines_simple};
use num::complex::Complex;

fn consume_and_map(
    points: impl Iterator< Item= Complex<f32>>
) {
    let _points = points.map(|x| x*x).collect::<Vec<Complex<f32>>>();
}

pub fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("gridlines iterator 1.0", |b| {
        b.iter(|| {
            let (points, _line_structure) = create_gridlines(
                black_box(1.0),
                black_box(-5.0),
                black_box(5.0),
                black_box(-5.0),
                black_box(5.0),
            );
            consume_and_map(points);
        })
    });
    c.bench_function("gridlines iterator 16.0", |b| {
        b.iter(|| {
            let (points, _line_structure) = create_gridlines(
                black_box(4.0),
                black_box(-5.0),
                black_box(5.0),
                black_box(-5.0),
                black_box(5.0),
            );
            consume_and_map(points);
        })
    });

    c.bench_function("gridlines vector 1.0", |b| b.iter(|| create_gridlines_simple(
        black_box(1.0), black_box(-5.0), black_box(5.0), black_box(-5.0), black_box(5.0)
    )));

    c.bench_function("gridlines vector 16.0", |b| b.iter(|| create_gridlines_simple(
        black_box(16.0), black_box(-5.0), black_box(5.0), black_box(-5.0), black_box(5.0)
    )));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
