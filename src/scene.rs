//! Everything a shared link shows, written to and read from the fragment of
//! the page URL, like `#ex=escher&color=source&view=0,0,600`.
//!
//! A link starts from an example and lists only what differs from it, so the
//! examples themselves get short links. Fields that are missing or don't parse
//! keep the example's value, so old or mangled links still open something.

use crate::route::Path;
use crate::examples::EXAMPLES;
use num::Complex;

/// Screen points per unit length in a fresh view.
pub const DEFAULT_PIXELS_PER_UNIT: f64 = 150.0;
/// Grid density of a fresh view: `2^resolution` lines per unit.
pub const DEFAULT_RESOLUTION: f64 = 2.0;

/// How grid lines are colored. The discriminants are the `COLORING_*`
/// constants in `src/grid.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Coloring {
    White,
    /// Which line of the source grid this is.
    Source,
    /// arg z of the source point, kept through the map.
    #[default]
    Phase,
    /// log |f'(z)|: how much `f` scales lengths here.
    Stretch,
    /// arg f'(z): how far `f` turns directions here.
    Rotation,
}

impl Coloring {
    pub const ALL: [Coloring; 5] = [
        Coloring::White,
        Coloring::Source,
        Coloring::Phase,
        Coloring::Stretch,
        Coloring::Rotation,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Coloring::White => "White",
            Coloring::Source => "Source",
            Coloring::Phase => "Phase",
            Coloring::Stretch => "Stretch",
            Coloring::Rotation => "Rotation",
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }

    fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| c.label().eq_ignore_ascii_case(slug))
    }
}

/// What the visualizer shows, as far as a link can carry it.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    /// Index into `EXAMPLES`: the one the rest starts from.
    pub example: usize,
    pub formula: String,
    pub parameter: Complex<f64>,
    /// x min, x max, y min, y max.
    pub grid: [f64; 4],
    pub resolution: f64,
    pub coloring: Coloring,
    /// Offset of the complex plane, in plane units.
    pub position: [f64; 2],
    pub pixels_per_unit: f64,
    /// The first and last stage selected (see `crate::route`), or `None` for
    /// all of them. Only the formula knows how many there are, so a link can
    /// ask for some that don't exist; the app selects them all then.
    pub stages: Option<(usize, usize)>,
    /// How the homotopy gets from its start to its end.
    pub path: Path,
    /// How far along the homotopy, from its start (0) to its end (1). `None`
    /// while the animation plays.
    pub homotopy: Option<f64>,
}

impl Scene {
    /// `EXAMPLES[index]` in a fresh view, animating.
    pub fn example(index: usize) -> Self {
        let example = &EXAMPLES[index];
        Scene {
            example: index,
            formula: example.formula.into(),
            parameter: example.parameter,
            grid: example.grid,
            resolution: DEFAULT_RESOLUTION,
            coloring: Coloring::default(),
            position: [0.0, 0.0],
            pixels_per_unit: DEFAULT_PIXELS_PER_UNIT,
            stages: None,
            path: Path::Straight,
            homotopy: None,
        }
    }

    /// The URL fragment (without `#`) for this scene; empty for the first example as it starts.
    pub fn to_fragment(&self) -> String {
        let base = Scene::example(self.example);
        let mut fields = Vec::new();
        if self.example != 0 {
            fields.push(format!("ex={}", EXAMPLES[self.example].slug));
        }
        let mut add = |key: &str, value: String, base_value: String| {
            if value != base_value {
                fields.push(format!("{key}={}", encode(&value)));
            }
        };
        add("f", self.formula.clone(), base.formula.clone());
        add("p", complex(self.parameter), complex(base.parameter));
        add("grid", list(&self.grid, 4), list(&base.grid, 4));
        add(
            "res",
            number(self.resolution, 2),
            number(base.resolution, 2),
        );
        add(
            "color",
            self.coloring.label().to_ascii_lowercase(),
            base.coloring.label().to_ascii_lowercase(),
        );
        add("view", self.view(), base.view());
        match self.stages {
            Some((first, last)) if first == last => fields.push(format!("stages={first}")),
            Some((first, last)) => fields.push(format!("stages={first}-{last}")),
            None => {}
        }
        match self.path {
            Path::Straight => {}
            Path::Stages => fields.push("path=stages".to_owned()),
            Path::Overlap => fields.push("path=overlap".to_owned()),
        }
        if let Some(t) = self.homotopy {
            fields.push(format!("t={}", number(t, 3)));
        }
        fields.join("&")
    }

    /// Reads a fragment written by [`Scene::to_fragment`], with or without the
    /// leading `#`. Never fails: whatever can't be read keeps its default.
    pub fn from_fragment(fragment: &str) -> Self {
        let fields: Vec<(String, String)> = fragment
            .trim_start_matches('#')
            .split('&')
            .filter_map(|field| field.split_once('='))
            .map(|(key, value)| (decode(key), decode(value)))
            .collect();
        let example = fields
            .iter()
            .find(|(key, _)| key == "ex")
            .and_then(|(_, slug)| EXAMPLES.iter().position(|e| e.slug == slug))
            .unwrap_or(0);

        let mut scene = Scene::example(example);
        for (key, value) in fields {
            match key.as_str() {
                "f" => scene.formula = value,
                "p" => {
                    if let Some([re, im]) = numbers(&value) {
                        scene.parameter = Complex::new(re, im);
                    }
                }
                "grid" => {
                    if let Some(grid) = numbers(&value) {
                        scene.grid = grid;
                    }
                }
                "res" => {
                    if let Some([resolution]) = numbers(&value) {
                        scene.resolution = resolution;
                    }
                }
                "color" => {
                    if let Some(coloring) = Coloring::from_slug(&value) {
                        scene.coloring = coloring;
                    }
                }
                "view" => {
                    if let Some([x, y, pixels_per_unit]) = numbers(&value) {
                        scene.position = [x, y];
                        scene.pixels_per_unit = pixels_per_unit;
                    }
                }
                "stages" => {
                    let (first, last) = value.split_once('-').unwrap_or((&value, &value));
                    scene.stages = first.parse().ok().zip(last.parse().ok());
                }
                "path" => {
                    match value.as_str() {
                        "stages" => scene.path = Path::Stages,
                        "overlap" => scene.path = Path::Overlap,
                        _ => {}
                    }
                }
                "t" => {
                    if let Some([t]) = numbers(&value) {
                        scene.homotopy = Some(t.clamp(0.0, 1.0));
                    }
                }
                _ => {}
            }
        }
        scene
    }

    /// Position and zoom, precise to a fraction of a screen point.
    fn view(&self) -> String {
        let decimals = (self.pixels_per_unit.max(1.0).log10().ceil() as usize + 1).max(2);
        // Far from 1 it takes every digit, in a form that keeps them short.
        let zoom = if (0.01..1e6).contains(&self.pixels_per_unit) {
            let decimals = (2.0 - self.pixels_per_unit.log10().ceil()).max(1.0) as usize;
            number(self.pixels_per_unit, decimals)
        } else {
            format!("{:e}", self.pixels_per_unit)
        };
        format!(
            "{},{},{zoom}",
            number(self.position[0], decimals),
            number(self.position[1], decimals),
        )
    }
}

/// `x` with up to `decimals` decimals, without trailing zeros.
fn number(x: f64, decimals: usize) -> String {
    let fixed = format!("{x:.decimals$}");
    let trimmed = if fixed.contains('.') {
        fixed.trim_end_matches('0').trim_end_matches('.')
    } else {
        &fixed
    };
    match trimmed {
        "-0" => "0".into(),
        other => other.into(),
    }
}

fn complex(z: Complex<f64>) -> String {
    list(&[z.re, z.im], 4)
}

fn list(values: &[f64], decimals: usize) -> String {
    let numbers: Vec<String> = values.iter().map(|&x| number(x, decimals)).collect();
    numbers.join(",")
}

/// Exactly `N` finite numbers separated by commas.
fn numbers<const N: usize>(text: &str) -> Option<[f64; N]> {
    let values: Vec<f64> = text
        .split(',')
        .map(|s| s.trim().parse::<f64>().ok().filter(|x| x.is_finite()))
        .collect::<Option<_>>()?;
    values.try_into().ok()
}

/// Percent-encodes `text` for a URL fragment, with `+` for spaces. Brackets
/// are encoded too, so a link survives being pasted into Markdown.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'.'
            | b'_'
            | b'~'
            | b'*'
            | b'/'
            | b',' => out.push(byte as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Undoes [`encode`]; also takes what browsers and people make of it
/// (unencoded characters, malformed escapes).
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let escaped = bytes
            .get(i + 1..i + 3)
            .filter(|hex| bytes[i] == b'%' && hex.iter().all(u8::is_ascii_hexdigit))
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match (escaped, bytes[i]) {
            (Some(byte), _) => {
                out.push(byte);
                i += 3;
                continue;
            }
            (None, b'+') => out.push(b' '),
            (None, byte) => out.push(byte),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_as_they_start_get_short_links() {
        assert_eq!(Scene::example(0).to_fragment(), "");
        let escher = EXAMPLES.iter().position(|e| e.slug == "escher").unwrap();
        assert_eq!(Scene::example(escher).to_fragment(), "ex=escher");
        for index in 0..EXAMPLES.len() {
            let scene = Scene::example(index);
            assert_eq!(Scene::from_fragment(&scene.to_fragment()), scene);
        }
    }

    #[test]
    fn a_changed_scene_round_trips() {
        let scene = Scene {
            formula: "p (1 - z)/(1 + z) + 2pi i ^ z".into(),
            parameter: Complex::new(-0.25, 1.5),
            grid: [-2.0, 0.5, -1.25, 3.0],
            resolution: 3.5,
            coloring: Coloring::Stretch,
            position: [0.125, -2.0],
            pixels_per_unit: 600.0,
            stages: Some((3, 3)),
            homotopy: Some(0.5),
            ..Scene::example(4)
        };
        let fragment = scene.to_fragment();
        assert!(!fragment.contains(['(', ')', ' ', '^']), "{fragment}");
        assert_eq!(Scene::from_fragment(&format!("#{fragment}")), scene);
    }

    #[test]
    fn a_view_zoomed_far_in_round_trips() {
        for (pixels_per_unit, x) in [(1e7, 0.30000012), (1e12, 0.300000000012), (1e200, 1e-195)] {
            let scene = Scene {
                pixels_per_unit,
                position: [x, -x],
                ..Scene::example(0)
            };
            let decoded = Scene::from_fragment(&scene.to_fragment());
            assert_eq!(decoded.pixels_per_unit, pixels_per_unit);
            // Off by less than a tenth of a point.
            let miss = (decoded.position[0] - x).abs() * pixels_per_unit;
            assert!(miss < 0.1, "{pixels_per_unit}: {miss}");
        }
    }

    #[test]
    fn a_view_zoomed_far_out_round_trips() {
        for pixels_per_unit in [0.5, 1e-3, 1e-9, 1e-30] {
            let scene = Scene {
                pixels_per_unit,
                ..Scene::example(0)
            };
            let decoded = Scene::from_fragment(&scene.to_fragment());
            let ratio = decoded.pixels_per_unit / pixels_per_unit;
            assert!((0.99..1.01).contains(&ratio), "{pixels_per_unit}: {ratio}");
        }
    }

    #[test]
    fn stages_are_left_out_when_all_are_selected() {
        assert!(!Scene::example(1).to_fragment().contains("stages"));
        let scene = Scene {
            stages: Some((2, 3)),
            path: Path::Stages,
            homotopy: Some(1.0),
            ..Scene::example(0)
        };
        assert_eq!(scene.to_fragment(), "stages=2-3&path=stages&t=1");
        assert_eq!(Scene::from_fragment("stages=2-3&path=stages&t=1"), scene);
        // While it plays there is no t, and the link plays it too.
        let playing = Scene { homotopy: None, ..scene };
        assert_eq!(Scene::from_fragment(&playing.to_fragment()), playing);
    }

    #[test]
    fn the_overlapping_path_round_trips() {
        let scene = Scene { path: Path::Overlap, homotopy: Some(1.0), ..Scene::example(0) };
        assert_eq!(scene.to_fragment(), "path=overlap&t=1");
        assert_eq!(Scene::from_fragment("path=overlap&t=1"), scene);
    }

    #[test]
    fn one_stage_is_written_as_one_number() {
        let scene = Scene { stages: Some((2, 2)), homotopy: Some(1.0), ..Scene::example(0) };
        assert_eq!(scene.to_fragment(), "stages=2&t=1");
        assert_eq!(Scene::from_fragment("stages=2&t=1"), scene);
    }

    #[test]
    fn links_from_before_stages_select_them_all() {
        for link in ["", "ex=escher&color=source", "f=exp%281/z%29&t=0.5&view=0,0,300"] {
            let scene = Scene::from_fragment(link);
            assert_eq!((scene.stages, scene.path), (None, Path::Straight), "{link}");
        }
    }

    #[test]
    fn stages_that_are_no_numbers_select_them_all() {
        for link in ["stages=two", "stages=-1", "stages=1.5", "stages=", "stages=1-", "stages=99999999999999999999999"] {
            assert_eq!(Scene::from_fragment(link).stages, None, "{link}");
        }
    }

    #[test]
    fn numbers_are_written_briefly() {
        let scene = Scene {
            parameter: Complex::new(1.23456, -0.0),
            homotopy: Some(1.0),
            ..Scene::example(0)
        };
        assert_eq!(scene.to_fragment(), "p=1.2346,0&t=1");
    }

    #[test]
    fn broken_links_keep_defaults() {
        let scene = Scene::from_fragment("ex=nope&p=1&grid=a,b,c,d&color=pink&t=NaN&view=1,2&&=&x");
        assert_eq!(scene, Scene::example(0));
        assert_eq!(Scene::from_fragment("t=7").homotopy, Some(1.0));
    }

    #[test]
    fn decoding_takes_what_browsers_send() {
        assert_eq!(decode("a+b%2Bc%28%29%zz%"), "a b+c()%zz%");
        assert_eq!(decode("%C3%A9%CF%80"), "éπ");
        assert_eq!(decode(&encode("π · z^2 & = #")), "π · z^2 & = #");
    }
}
