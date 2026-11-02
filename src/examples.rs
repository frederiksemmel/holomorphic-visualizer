//! The functions on offer in the control window, each with a grid and
//! parameter that show it well.

use num::Complex;

/// A function to look at, with a grid and parameter that show it well.
pub struct Example {
    pub name: &'static str,
    /// Names the example in shared links (see [`crate::scene`]).
    pub slug: &'static str,
    /// What to type for f(z); see `src/expr.rs` for the syntax.
    pub formula: &'static str,
    pub about: &'static str,
    /// Further reading: (link text, URL).
    pub links: &'static [(&'static str, &'static str)],
    /// x min, x max, y min, y max.
    pub grid: [f32; 4],
    pub parameter: Complex<f32>,
}

/// The functions on offer in the control window. The first one is what the
/// visualizer opens with, and what links without `ex=` start from.
pub const EXAMPLES: [Example; 6] = [
    Example {
        name: "Exponential",
        slug: "exp",
        formula: "exp(p * z)",
        about: "Horizontal lines become rays, vertical lines become circles. \
                The strip of height 2π wraps once around the origin.",
        links: &[],
        grid: [-1.0, 1.0, -3.0, 3.0],
        parameter: Complex::new(1.0, 0.0),
    },
    Example {
        name: "Möbius",
        slug: "mobius",
        formula: "p * (1 - z) / (1 + z)",
        about: "A Möbius transformation: it maps lines and circles to lines and circles.",
        links: &[],
        grid: [0.0, 1.0, 0.0, 1.0],
        parameter: Complex::new(1.0, 0.0),
    },
    Example {
        name: "Power",
        slug: "power",
        formula: "z^p",
        about: "For real p it multiplies angles at the origin by p. \
                Try a tall rectangle just right of 0.",
        links: &[],
        grid: [0.5, 1.0, -1.0, 1.0],
        parameter: Complex::new(3.0, 0.0),
    },
    Example {
        name: "Sine",
        slug: "sine",
        formula: "p * sin(z)",
        about: "Vertical lines become hyperbolas, horizontal lines ellipses, \
                all with foci at ±p.",
        links: &[],
        grid: [-1.5, 1.5, -1.0, 1.0],
        parameter: Complex::new(1.0, 0.0),
    },
    Example {
        name: "Joukowski",
        slug: "joukowski",
        formula: "z + p / z",
        about: "The Joukowski map, used to model airfoils. It folds the circle |z| = √|p| \
                onto a segment; inside and outside that circle each cover the rest \
                of the plane once.",
        links: &[],
        grid: [-2.0, 2.0, -2.0, 2.0],
        parameter: Complex::new(1.0, 0.0),
    },
    Example {
        name: "Escher",
        slug: "escher",
        formula: "exp(2pi i z / (2pi i + log(p * e^6)))",
        about: "M. C. Escher's Print Gallery (1956). The grid is his straight picture, \
                which contains itself scaled down, in log coordinates: vertical lines \
                are circles around its centre, horizontal lines are rays. f curls it so \
                that one turn around the centre zooms in by p e⁶. Escher used 256 \
                (p ≈ 0.635); with p = 1 the circles meet up after every turn.",
        links: &[
            (
                "Print Gallery on Wikipedia",
                "https://en.wikipedia.org/wiki/Print_Gallery_(M._C._Escher)",
            ),
            (
                "de Smit & Lenstra: The Mathematical Structure of Escher's Print Gallery",
                "https://pub.math.leidenuniv.nl/~smitbde/papers/2003-de_smit-lenstra-escher.pdf",
            ),
        ],
        grid: [-12.0, -1.0, -3.0, 3.25],
        parameter: Complex::new(1.0, 0.0),
    },
];
