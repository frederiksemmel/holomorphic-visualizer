use bevy::asset::{Assets, uuid_handle};
use bevy::input::{ButtonInput, mouse::MouseScrollUnit, touch::TouchPhase};
use bevy::platform::time::Instant;
use conformal_visualizer::axes::Step;
use conformal_visualizer::borders::{self, Border};
use conformal_visualizer::examples::EXAMPLES;
use conformal_visualizer::expr::{self, Function, Homotopy, ParseError};
use conformal_visualizer::scene::{Coloring, DEFAULT_PIXELS_PER_UNIT, DEFAULT_RESOLUTION, Scene};
use conformal_visualizer::patch::Patch;
use conformal_visualizer::route::{Mix, Path, Route};
use conformal_visualizer::{COMPLEX_WGSL, GridLayout, clip_to_view, morph, readout};
use nannou::prelude::render::ShaderModel;
use nannou::prelude::*;
use bevy::math::{DVec2, UVec3, dvec2, uvec3};
use num::Complex;
use std::sync::Arc;
use std::time::Duration;

/// Points per scroll "line" for mice that report wheel clicks instead of pixels.
const POINTS_PER_LINE: f32 = 24.0;
/// Finest grid the controls offer; below the library's hard cap
/// (`conformal_visualizer::MAX_RESOLUTION`) to keep the line count sane.
const MAX_UI_RESOLUTION: f64 = 6.0;
/// The grid edges stay within `[-DOMAIN_LIMIT, DOMAIN_LIMIT]`. Not a limit
/// anyone should meet: it only keeps the arithmetic finite, and f32 has long
/// stopped telling neighbouring grid lines apart out here.
const DOMAIN_LIMIT: f64 = 1e12;
/// Fraction of an edge's size that dragging its field moves it per screen
/// point, so it covers tiny and huge values alike.
const EDGE_DRAG_SPEED: f64 = 0.005;
/// Zoom limits, in screen points per unit, there only to keep the arithmetic
/// finite: what the zoom runs into first is the precision of f64, about 1e15
/// at |z| = 1 and much further near the origin. Zoomed out, the GPU's f32 sets
/// the limit, as nothing there goes through a `Patch`.
const SCALE_RANGE: std::ops::RangeInclusive<f64> = 1e-30..=1e300;
/// Zoom, times the size of what is at the window centre, from which the grid is
/// drawn from a `Patch`. There f32 is still good to a fraction of a pixel.
const PATCH_ABOVE: f64 = 1e5;
/// Exponent of the zoom per screen point of Ctrl + scroll or of dragging the
/// zoom field. Its range spans too many decades for a slider's track.
const ZOOM_PER_POINT: f64 = 0.005;
/// Scrolling or mouse travel, in screen points, across the whole homotopy range
/// at the default zoom (see [`Model::homotopy_travel`]).
const HOMOTOPY_TRAVEL: f64 = 900.0;
/// Seconds that holding ← or → takes across the whole homotopy range at the
/// default zoom.
const ARROW_SECONDS: f32 = 2.0;
/// Longest time step of a motion driven by a held key. The app sleeps
/// between inputs, so the frame that starts the motion can follow a long pause.
const MAX_KEY_STEP: f32 = 1.0 / 30.0;
/// Screen points of drag or scroll per resolution level.
const RESOLUTION_TRAVEL: f32 = 50.0;
/// Shift + scroll moves a grid border by one grid square per this many
/// screen points: a square's size on screen at the default zoom and resolution.
const SCROLL_PER_SQUARE: f64 = DEFAULT_PIXELS_PER_UNIT / (1 << DEFAULT_RESOLUTION as u32) as f64;
/// At most this many squares per scroll event, so a flick doesn't throw the
/// border across the plane.
const MAX_SCROLL_SQUARES: f64 = 4.0;
/// The control window fades to this while one of its sliders is dragged, to
/// show the result underneath: on a phone it covers most of the canvas.
const HELD_PANEL_OPACITY: f32 = 0.2;
/// How long that fade takes.
const PANEL_FADE_SECONDS: f32 = 0.1;
/// Shift slows dragging p (on its pad or with the right button) to this.
const FINE: f32 = 0.2;
/// Straight segments per unit length of grid line, so lines bend smoothly under `f`.
const MAX_DENSITY: f64 = 2048.0;
/// Newton steps to follow a patch's grid point from one frame to the next.
const PATCH_NEWTON_STEPS: usize = 8;
/// Upper bound on segments drawn per frame; large grids get coarser sampling.
const MAX_SEGMENTS: u32 = 1_000_000;
/// In screen points.
const LINE_WIDTH: f32 = 1.5;
/// Stretch factors beyond 2^±STRETCH_OCTAVES get the extreme color. Sent to
/// `src/grid.wgsl` in `GridParams`, so the legend and the shader agree.
const STRETCH_OCTAVES: f32 = 2.0;
/// Seconds to keep redrawing after the grid shader changes. Bevy doesn't draw
/// the grid while it recompiles, and a tap on a stage is one input: with
/// nothing moving, the app would sleep before the new shader is ready.
const SHADER_SETTLE: f64 = 1.0;
/// Seconds between updates of the page URL. Browsers limit how often a page
/// may rewrite its URL (Safari to 100 times in 30 s), and a slider drag
/// changes the scene every frame.
const LINK_INTERVAL: f64 = 0.4;
/// The background grid of the plane: major lines about this many screen
/// points apart, at a step of 1, 2 or 5 times a power of ten.
const MAJOR_SPACING: f64 = 120.0;
/// Its lines, as grays (sRGB) on the background (0.02). Opaque rather than
/// translucent, so crossings don't add up to dots.
const MINOR_GRAY: f32 = 0.055;
const MAJOR_GRAY: f32 = 0.085;
const AXIS_GRAY: f32 = 0.14;
/// In screen points.
const PLANE_LINE_WIDTH: f32 = 1.0;
/// The numbers on its axes: size in points, color, distance from the axis,
/// and from the window edge when the axis is off screen.
const LABEL_SIZE: f32 = 11.0;
const LABEL_COLOR: egui::Color32 = egui::Color32::from_gray(100);
const LABEL_GAP: f32 = 4.0;
const LABEL_MARGIN: f32 = 4.0;
/// The hover readout shows once the cursor has rested this long.
const READOUT_DELAY: Duration = Duration::from_millis(500);
/// The app wakes this much after the readout is due, and shows it this
/// much before: the wake-up is timed from the start of the frame that
/// sets it, a little before `update` reads the clock.
const WAKE_SLACK: Duration = Duration::from_millis(20);
/// Most source points the readout lists.
const READOUT_SOURCES: usize = 3;
/// How close, in screen points, the image of a source point must come to
/// the cursor.
const READOUT_TOLERANCE: f64 = 0.25;
/// Text size of the readout card, in points.
const READOUT_SIZE: f32 = 14.0;
/// Its corner sits this many points from the cursor.
const READOUT_OFFSET: f32 = 18.0;
/// Dots at the cursor and at the source points, and the lines that join
/// them: radii and width in points.
const MARK_COLOR: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(255, 255, 255, 110);
const LINK_COLOR: egui::Color32 = egui::Color32::from_rgba_unmultiplied_const(255, 255, 255, 45);
const CURSOR_MARK_RADIUS: f32 = 2.0;
const SOURCE_MARK_RADIUS: f32 = 2.5;
const LINK_WIDTH: f32 = 1.0;
/// Fonts for the axis numbers and the readout (assets/fonts, SIL OFL):
/// KaTeX's upright and italic, the look of typeset math.
const MATH_FONT: &str = "math";
const MATH_ITALIC_FONT: &str = "math italic";
/// Where the browser version lives; links copied in the desktop app point there.
#[cfg(not(target_arch = "wasm32"))]
const WEB_URL: &str = "https://frederiksemmel.github.io/holomorphic-visualizer/";

const GRID_SHADER: Handle<Shader> = uuid_handle!("b69f3f88-de67-4239-b652-1b805606f44f");

fn main() {
    nannou::app(model)
        .add_plugin(GridShaderPlugin)
        .shader_model::<GridShader>()
        .update(update)
        .run();
}

struct GridShaderPlugin;

impl bevy::app::Plugin for GridShaderPlugin {
    fn build(&self, app: &mut bevy::app::App) {
        let function = expr::parse(EXAMPLES[0].formula).expect("examples parse");
        set_grid_shader(app.world_mut(), &expr::chain_wgsl(&function.chain()));
    }
}

/// The grid shader with `functions` (generated by `expr::chain_wgsl`).
fn grid_shader_source(functions: &str) -> String {
    format!("{}\n{COMPLEX_WGSL}\n{functions}", include_str!("grid.wgsl"))
}

/// Puts the grid shader with `functions` in place. Bevy rebuilds the
/// pipelines that use it.
fn set_grid_shader(world: &mut World, functions: &str) {
    let source = grid_shader_source(functions);
    world
        .resource_mut::<Assets<Shader>>()
        .insert(GRID_SHADER.id(), Shader::from_wgsl(source, "grid.wgsl"))
        .expect("a UUID handle is always valid");
}

/// Draws the grid with `src/grid.wgsl`, which maps it through `f` on the GPU.
#[derive(Asset, TypePath, AsBindGroup, Clone, Debug, Default)]
struct GridShader {
    #[uniform(0)]
    params: GridParams,
}

impl ShaderModel for GridShader {
    fn vertex_shader() -> ShaderRef {
        GRID_SHADER.into()
    }

    fn fragment_shader() -> ShaderRef {
        GRID_SHADER.into()
    }
}

/// Field order and types must match `GridParams` in `src/grid.wgsl`.
#[derive(Clone, Copy, Debug, Default, ShaderType)]
struct GridParams {
    parameter: Vec2,
    offset: Vec2,
    grid_min: Vec2,
    grid_max: Vec2,
    spacing: f32,
    h_lines: u32,
    h_segments: u32,
    v_segments: u32,
    pixels_per_unit: f32,
    peek: u32,
    stages: UVec3,
    weights: Vec3,
    color_stage: u32,
    line_width: f32,
    coloring: u32,
    stretch_octaves: f32,
    domain_min: Vec2,
    domain_max: Vec2,
    half_view: Vec2,
    patched: u32,
    units_per_u: f32,
    patch_source: Vec2,
    lines_from: Vec2,
    cubic0: Vec2,
    cubic1: Vec2,
    cubic2: Vec2,
    cubic3: Vec2,
}

/// The page URL, which carries the scene (see `conformal_visualizer::scene`).
#[cfg(target_arch = "wasm32")]
mod link {
    /// The fragment of the page URL, without `#`.
    pub fn fragment() -> String {
        web_sys::window()
            .and_then(|window| window.location().hash().ok())
            .map(|hash| hash.trim_start_matches('#').to_owned())
            .unwrap_or_default()
    }

    /// Puts `fragment` in the page URL without adding a history entry, and
    /// without the `hashchange` event that `index.html` reloads the page on.
    pub fn set_fragment(fragment: &str) {
        let Some(window) = web_sys::window() else {
            return;
        };
        let url = if fragment.is_empty() {
            let location = window.location();
            location.pathname().unwrap_or_default() + &location.search().unwrap_or_default()
        } else {
            format!("#{fragment}")
        };
        if let Ok(history) = window.history() {
            // It throws if called too often; the next update tries again.
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&url));
        }
    }

    /// A link to this page showing `fragment`.
    pub fn url(fragment: &str) -> String {
        let href = web_sys::window()
            .and_then(|window| window.location().href().ok())
            .unwrap_or_default();
        let page = href.split('#').next().unwrap_or_default();
        super::with_fragment(page, fragment)
    }
}

/// The desktop app takes a link (or just its `#…` part) as its argument, and
/// copies links to the browser version.
#[cfg(not(target_arch = "wasm32"))]
mod link {
    pub fn fragment() -> String {
        let argument = std::env::args().nth(1).unwrap_or_default();
        match argument.split_once('#') {
            Some((_, fragment)) => fragment.to_owned(),
            None => argument,
        }
    }

    pub fn set_fragment(_fragment: &str) {}

    pub fn url(fragment: &str) -> String {
        super::with_fragment(super::WEB_URL, fragment)
    }
}

fn with_fragment(page: &str, fragment: &str) -> String {
    if fragment.is_empty() {
        page.to_owned()
    } else {
        format!("{page}#{fragment}")
    }
}

/// The animation along the homotopy and back (see `conformal_visualizer::morph`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Morph {
    Off,
    /// Starts on the next update, from this homotopy.
    Starting(f64),
    /// Playing; the app time at which the animation was at its start.
    Since(f64),
}

/// What a mouse drag on the canvas does, decided when the button goes down.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Pan,
    Parameter,
    Resolution,
    /// Moves the border Shift holds, like Shift + scroll.
    HeldBorder,
    /// Slides the source grid (Alt).
    Grid,
}

/// Where the cursor came to rest, for the hover readout.
struct Rest {
    /// Cursor position (canvas points) when it last moved.
    at: Vec2,
    /// When it last moved, or something else (a scroll, a key, a button)
    /// happened that the readout should wait out.
    since: Instant,
    /// The last thing to move the pointer was a finger: touch screens have
    /// no hover, so there is no readout.
    touch: bool,
}

struct Model {
    window: Entity,
    /// Index into `EXAMPLES`: the last one picked.
    example: usize,
    /// The formula for f(z) in the text field.
    formula: String,
    /// Why `formula` can't be drawn; the last one that could stays on screen.
    formula_error: Option<ParseError>,
    /// The stages of the last formula that worked, after the identity: the
    /// functions the shader draws, for the CPU (see `Function::chain`).
    chain: Vec<Function>,
    /// Which of them the homotopy runs over, and how.
    route: Route,
    /// The stage that a tap or Shift + digit makes a range with.
    anchor: Option<usize>,
    /// WGSL for a new `chain`, to be put in the grid shader on the next update.
    new_function: Option<String>,
    /// App time until which to keep redrawing (see [`SHADER_SETTLE`]).
    settling_until: f64,
    /// Screen points per unit length in the complex plane.
    pixels_per_unit: f64,
    /// Offset of the complex plane, in plane units.
    position: DVec2,
    /// Grid density: `2^resolution` lines per unit.
    resolution: f64,
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    parameter: Complex<f64>,
    /// How far along the `route`: its start (0) to its end (1).
    homotopy: f64,
    morph: Morph,
    coloring: Coloring,
    /// The fragment last put in the page URL, and the app time it was put there.
    link_written: (String, f64),
    /// The fragment of the link "Copy link" last copied.
    link_copied: Option<String>,
    /// Redrawing every frame (animating, or a URL update is due) rather than on input.
    continuous: bool,
    /// When the app is set to wake up without input, to show the readout.
    wake_at: Option<Instant>,
    rest: Rest,
    /// Mouse button held on the canvas, what it does, and the cursor position
    /// at the last update.
    drag: Option<(MouseButton, Drag, Vec2)>,
    /// Space is down and no scroll or drag has used it yet, so letting go of
    /// it plays or pauses.
    space_tap: bool,
    /// The border Shift + scroll moves, and the map's derivative where it was
    /// drawn at the cursor, both taken when Shift went down and kept until it
    /// comes up. Found again on every scroll, the border would switch to
    /// another one as it moved away from the cursor, and the direction and
    /// speed of the scroll would change under the fingers.
    scrolled_border: Option<(Border, Complex<f64>)>,
    /// Shift was down at the last update.
    shift_held: bool,
    /// The "hold" button in the control window is pressed.
    peek_button: bool,
    /// Show the source grid for now (hold button or X).
    peek: bool,
    /// Cursor position at the last update, for key + mouse-move gestures.
    last_mouse: Vec2,
    /// Half the window size, in screen points.
    half_view: Vec2,
    /// Fingers on the canvas (by touch id) and where they were last seen.
    touches: Vec<(u64, Vec2)>,
    /// Screen area of the control window, in points from the top left.
    panel: egui::Rect,
    /// The control window shows its controls, not just its header. Until the
    /// user taps the header (`None`), it is open on wide screens only; that is
    /// decided every frame, as the window gets its real size a few frames in.
    panel_open: Option<bool>,
    /// A slider or other dragged value in the control window was held at the
    /// last update.
    slider_held: bool,
    /// The grid point of the last `Patch`. The next one is usually right next
    /// to it, which spares the search through the whole grid.
    patch_seed: Option<Complex<f64>>,
}

impl Model {
    /// The whole grid, on screen or not.
    fn full_layout(&self) -> GridLayout {
        let (min, max) = self.domain();
        Self::layout(self.resolution, min, max)
    }

    /// The grid to draw. Unmapped, only the part inside the window can be
    /// seen, so only that part is laid out and the whole segment budget goes
    /// to it: zoomed in, the lines stay smooth. Mapped, lines from anywhere can
    /// land on screen; the shader drops the ones that do not.
    fn grid_layout(&self) -> GridLayout {
        let (mut min, mut max) = self.domain();
        if self.unmapped() {
            let half = self.half_view.as_dvec2() / self.pixels_per_unit;
            let (view_min, view_max) = (-half - self.position, half - self.position);
            (min, max) = clip_to_view(
                self.resolution,
                min,
                max,
                Complex::new(view_min.x, view_min.y),
                Complex::new(view_max.x, view_max.y),
            );
        }
        Self::layout(self.resolution, min, max)
    }

    fn domain(&self) -> (Complex<f64>, Complex<f64>) {
        (
            Complex::new(self.x_min, self.y_min),
            Complex::new(self.x_max, self.y_max),
        )
    }

    fn layout(resolution: f64, min: Complex<f64>, max: Complex<f64>) -> GridLayout {
        GridLayout::new(
            resolution,
            min.re,
            max.re,
            min.im,
            max.im,
            MAX_DENSITY,
            MAX_SEGMENTS,
        )
    }

    /// How far to blend from the start of the homotopy towards its end.
    /// The stages there are, not counting the identity.
    fn count(&self) -> usize {
        self.chain.len() - 1
    }

    /// What shows right now.
    fn mix(&self) -> Mix {
        self.route.mix(self.homotopy)
    }

    fn ends(&self) -> Homotopy<'_> {
        Homotopy::of(&self.chain, self.mix().0)
    }

    /// The stage on screen, if it is exactly one.
    fn shown_stage(&self) -> Option<usize> {
        self.mix().exact()
    }

    /// The grid is drawn as it is in the plane. A blend of 0 alone doesn't
    /// say so: from stage 3 on the homotopy starts from a mapped grid.
    fn unmapped(&self) -> bool {
        self.peek || self.ends().unmapped()
    }

    /// Where the grid point `z` is drawn in the plane, as `g` in
    /// `src/grid.wgsl` computes it.
    fn map(&self, z: Complex<f64>) -> Complex<f64> {
        if self.peek {
            z
        } else {
            self.ends().map(z, self.parameter)
        }
    }

    /// The derivative of [`Model::map`].
    fn map_derivative(&self, z: Complex<f64>) -> Complex<f64> {
        if self.peek {
            Complex::new(1.0, 0.0)
        } else {
            self.ends().derivative(z, self.parameter)
        }
    }

    /// Where the grid point `z` is drawn on screen, in points.
    fn grid_to_screen(&self, z: Complex<f64>) -> Complex<f64> {
        (self.map(z) + Complex::new(self.position.x, self.position.y)) * self.pixels_per_unit
    }

    fn grid(&self) -> [f64; 4] {
        [self.x_min, self.x_max, self.y_min, self.y_max]
    }

    /// The cubic that stands in for the map in the shader, when zoomed in far
    /// enough that the map itself would be off by pixels and bends so little
    /// across the window that a cubic will do. With several points landing at
    /// the window centre (z² has two), it follows one of them.
    fn patch(&self) -> Option<Patch> {
        let ppu = self.pixels_per_unit;
        let position = Complex::new(self.position.x, self.position.y);
        let centre = -position;
        if ppu * centre.norm().max(1.0) < PATCH_ABOVE {
            return None;
        }
        let source = self.point_at(centre)?;
        let p = self.parameter;
        let jet = if self.peek {
            Function::identity().jet(source, p)
        } else {
            self.ends().jet(source, p)
        };
        let reach = self.half_view.length() as f64;
        Patch::new(source, jet, position, ppu, reach, &self.full_layout(), MAX_SEGMENTS)
    }

    /// A grid point that lands on `w`: one next to the last patch's if there
    /// is one, otherwise the first found. It may lie just outside the grid,
    /// with the grid's edge in view.
    fn point_at(&self, w: Complex<f64>) -> Option<Complex<f64>> {
        if self.unmapped() {
            return Some(w);
        }
        let tolerance = (0.25 / self.pixels_per_unit).max(8.0 * f64::EPSILON * w.norm().max(1.0));
        let newton = |mut z: Complex<f64>| {
            for _ in 0..PATCH_NEWTON_STEPS {
                let miss = self.map(z) - w;
                if miss.norm() <= tolerance {
                    return Some(z);
                }
                z -= miss / self.map_derivative(z);
                if !(z.re.is_finite() && z.im.is_finite()) {
                    return None;
                }
            }
            None
        };
        self.patch_seed.and_then(newton).or_else(|| {
            let (min, max) = self.domain();
            let slack = 0.25 * (max - min);
            let map = |z| self.map(z);
            let derivative = |z| self.map_derivative(z);
            let (found, _) = readout::preimages(map, derivative, (min - slack, max + slack), w, tolerance, 1);
            found.first().copied()
        })
    }

    /// The grid border drawn nearest the screen point `cursor`, and the grid
    /// point on it that lands there.
    fn nearest_border(&self, cursor: Vec2) -> Option<(Border, Complex<f64>)> {
        borders::nearest_border(self.grid(), &|z| self.grid_to_screen(z), complex(cursor))
    }

    /// Moves `border` by `step` grid units. Like `move_edges`, it may pass
    /// the opposite border, which empties the grid until it comes back.
    fn move_border(&mut self, border: Border, step: f64) {
        let edge = match border {
            Border::XMin => &mut self.x_min,
            Border::XMax => &mut self.x_max,
            Border::YMin => &mut self.y_min,
            Border::YMax => &mut self.y_max,
        };
        *edge = (*edge + step).clamp(-DOMAIN_LIMIT, DOMAIN_LIMIT);
    }

    /// The border drawn nearest `cursor`, and the map's derivative where it
    /// passes the cursor, for Shift + scroll to hold on to.
    fn grab_for_scroll(&self, cursor: Vec2) -> Option<(Border, Complex<f64>)> {
        let (border, z) = self.nearest_border(cursor)?;
        Some((border, self.map_derivative(z)))
    }

    /// Moves the border Shift holds by a scroll of `delta` screen points,
    /// measured across it as it was drawn when Shift went down. A scroll moves
    /// it by grid squares, not with the fingers on screen: where the map draws
    /// the squares small, following the fingers would race across them, and
    /// where it draws them large, crawl.
    fn scroll_held_border(&mut self, cursor: Vec2, delta: Vec2, wheel: bool) {
        if self.scrolled_border.is_none() {
            // Shift went down in the same frame as this scroll.
            self.scrolled_border = self.grab_for_scroll(cursor);
        }
        if let Some((border, derivative)) = self.scrolled_border {
            let across = border.scroll_across(derivative, complex(delta), wheel);
            let squares = (across / SCROLL_PER_SQUARE).clamp(-MAX_SCROLL_SQUARES, MAX_SCROLL_SQUARES);
            self.move_border(border, squares * self.full_layout().spacing);
        }
    }

    /// Moves the border Shift holds by a drag of `delta` screen points: as
    /// fast as the mouse moves across it where it was drawn when the drag
    /// started.
    fn drag_held_border(&mut self, delta: Vec2) {
        if let Some((border, derivative)) = self.scrolled_border {
            let across = border.scroll_across(derivative, complex(delta), false);
            let step = across / (derivative.norm() * self.pixels_per_unit);
            if step.is_finite() {
                self.move_border(border, step.clamp(-borders::MAX_STEP, borders::MAX_STEP));
            }
        }
    }

    /// Slides the source grid by `delta` screen points: the square itself,
    /// whatever the map then does to it.
    fn move_grid(&mut self, delta: Vec2) {
        let step = delta.as_dvec2() * self.edge_units_per_point();
        self.move_border(Border::XMin, step.x);
        self.move_border(Border::XMax, step.x);
        self.move_border(Border::YMin, step.y);
        self.move_border(Border::YMax, step.y);
    }

    /// Moves the homotopy by `amount` screen points of travel (see
    /// [`Model::homotopy_travel`]), stopping the animation.
    fn scrub(&mut self, amount: f32) {
        self.morph = Morph::Off;
        self.homotopy = (self.homotopy + amount as f64 / self.homotopy_travel()).clamp(0.0, 1.0);
    }

    /// Makes the grid finer by `amount` screen points of drag or scroll
    /// (coarser, if negative).
    fn refine(&mut self, amount: f32) {
        self.resolution =
            (self.resolution + (amount / RESOLUTION_TRAVEL) as f64).clamp(0.0, MAX_UI_RESOLUTION);
    }

    /// Shows `EXAMPLES[index]` with its own grid and parameter, bending from
    /// the source grid.
    fn set_example(&mut self, index: usize) {
        let example = &EXAMPLES[index];
        self.example = index;
        self.set_formula(example.formula.into());
        [self.x_min, self.x_max, self.y_min, self.y_max] = example.grid;
        self.parameter = example.parameter;
        self.reset_view();
        self.homotopy = 0.0;
        self.morph = Morph::Starting(0.0);
    }

    /// What a link to the current view carries.
    fn scene(&self) -> Scene {
        Scene {
            example: self.example,
            formula: self.formula.clone(),
            parameter: self.parameter,
            grid: [self.x_min, self.x_max, self.y_min, self.y_max],
            resolution: self.resolution,
            coloring: self.coloring,
            position: self.position.to_array(),
            pixels_per_unit: self.pixels_per_unit,
            stages: (!self.route.is_all(self.count())).then_some((self.route.first, self.route.last)),
            path: self.route.path,
            homotopy: (self.morph == Morph::Off).then_some(self.homotopy),
        }
    }

    /// Shows what a link carries, within the limits the controls have.
    fn set_scene(&mut self, scene: Scene) {
        self.example = scene.example;
        self.set_formula(scene.formula);
        [self.x_min, self.x_max, self.y_min, self.y_max] =
            scene.grid.map(|edge| edge.clamp(-DOMAIN_LIMIT, DOMAIN_LIMIT));
        self.parameter = scene.parameter;
        self.resolution = scene.resolution.clamp(0.0, MAX_UI_RESOLUTION);
        self.coloring = scene.coloring;
        self.position = DVec2::from_array(scene.position);
        self.pixels_per_unit = scene
            .pixels_per_unit
            .clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        self.route.path = scene.path;
        // A formula that doesn't parse has no stages to ask for.
        if let Some((first, last)) = scene.stages.filter(|_| self.formula_error.is_none())
            && first >= 1
            && first <= last
            && last <= self.count()
        {
            (self.route.first, self.route.last) = (first, last);
        }
        (self.homotopy, self.morph) = match scene.homotopy {
            Some(homotopy) => (homotopy, Morph::Off),
            None => (0.0, Morph::Starting(0.0)),
        };
    }

    /// Starts the animation from the current homotopy, or stops it where it is.
    fn toggle_morph(&mut self) {
        self.morph = match self.morph {
            Morph::Off => Morph::Starting(self.homotopy),
            _ => Morph::Off,
        };
    }

    /// Shows all of `formula`, if it parses. Either way all stages end up
    /// selected: the ones selected may be about to stop existing.
    fn set_formula(&mut self, formula: String) {
        match expr::parse(&formula) {
            Ok(function) => {
                self.chain = function.chain();
                self.new_function = Some(expr::chain_wgsl(&self.chain));
                self.formula_error = None;
            }
            Err(error) => self.formula_error = Some(error),
        }
        self.route = Route::all(self.count(), self.route.path);
        self.anchor = None;
        self.formula = formula;
    }

    /// Selects the stages `first..=last`, keeping the picture if the new
    /// selection shows what is on screen: going on to the next stage starts
    /// where the last one finished. Otherwise it shows finished, so that
    /// scrubbing back reveals where it came from. If the animation is playing
    /// it goes on playing, from the start of the selection.
    fn select(&mut self, first: usize, last: usize) {
        let route = Route { first, last, ..self.route };
        if route == self.route {
            return;
        }
        let shown = self.shown_stage();
        self.route = route;
        (self.homotopy, self.morph) = match self.morph {
            Morph::Off => {
                let kept = shown.and_then(|stage| route.position_of(stage));
                (kept.unwrap_or(1.0), Morph::Off)
            }
            _ => (0.0, Morph::Starting(0.0)),
        };
    }

    /// Selects all the stages: the whole homotopy.
    fn select_all(&mut self) {
        self.select(1, self.count());
        self.anchor = None;
    }

    /// Selects only `stage`, if it exists, and starts a range there.
    fn pick(&mut self, stage: usize) {
        if (1..=self.count()).contains(&stage) {
            self.select(stage, stage);
            self.anchor = Some(stage);
        }
    }

    /// Selects from the anchor to `stage`, if it exists.
    fn extend_to(&mut self, stage: usize) {
        if (1..=self.count()).contains(&stage) {
            let anchor = *self.anchor.get_or_insert(self.route.first);
            self.select(anchor.min(stage), anchor.max(stage));
        }
    }

    /// A tap picks a stage, and the next tap makes a range with it, as in a
    /// date picker: a phone has no Shift.
    fn tap(&mut self, stage: usize) {
        match self.anchor {
            Some(anchor) => {
                self.select(anchor.min(stage), anchor.max(stage));
                self.anchor = None;
            }
            None => self.pick(stage),
        }
    }

    /// Moves the selection by `delta` stages, or only its end. It stays as
    /// long as it is, and on the stages there are.
    fn move_selection(&mut self, delta: isize, end_only: bool) {
        let Route { first, last, .. } = self.route;
        let moved = |stage: usize| stage.checked_add_signed(delta);
        let selection = if end_only {
            moved(last).filter(|&last| last >= first).map(|last| (first, last))
        } else {
            moved(first).filter(|&first| first >= 1).zip(moved(last))
        };
        if let Some((first, last)) = selection.filter(|&(_, last)| last <= self.count()) {
            self.select(first, last);
            self.anchor = None;
        }
    }

    /// Plays the selection straight or through its stages, keeping the
    /// picture when it can.
    fn set_path(&mut self, path: Path) {
        let shown = self.shown_stage();
        self.route.path = path;
        if let Some(t) = shown.and_then(|stage| self.route.position_of(stage)) {
            self.homotopy = t;
        }
        if self.morph != Morph::Off {
            self.morph = Morph::Starting(self.homotopy);
        }
    }

    /// What the readout and the legend call stage `stage`.
    fn stage_name(&self, stage: usize) -> String {
        if stage == self.count() { "f".into() } else { format!("g{stage}") }
    }

    fn reset_parameter(&mut self) {
        self.parameter = EXAMPLES[self.example].parameter;
    }

    fn reset_view(&mut self) {
        self.pixels_per_unit = DEFAULT_PIXELS_PER_UNIT;
        self.position = DVec2::ZERO;
    }

    /// Zooms by `factor`, keeping the plane point under `anchor` (screen points) in place.
    fn zoom(&mut self, factor: f64, anchor: Vec2) {
        let anchor = anchor.as_dvec2();
        let under_anchor = anchor / self.pixels_per_unit - self.position;
        self.pixels_per_unit =
            (self.pixels_per_unit * factor).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        self.position = anchor / self.pixels_per_unit - under_anchor;
    }

    /// Grid units per screen point for gestures that drag the grid edges.
    /// Unmapped, an edge follows the cursor. Mapped, the zoom says nothing
    /// about how far a source edge moves on screen (and far zoomed in, 1:1
    /// would barely move it), so edges move at the default zoom's rate.
    fn edge_units_per_point(&self) -> f64 {
        if self.unmapped() {
            1.0 / self.pixels_per_unit
        } else {
            1.0 / DEFAULT_PIXELS_PER_UNIT
        }
    }

    /// Screen points of scrolling or mouse travel across the whole homotopy
    /// range. A step of the homotopy moves each point by a fixed distance in
    /// the plane, so zoomed in by k it moves k times as far on screen: the
    /// travel grows with the zoom to keep that motion as fast as at the
    /// default zoom. Zoomed out it stays put, or a flick would jump from z to f.
    /// Each section of the route gets the whole of it, as it does in time.
    fn homotopy_travel(&self) -> f64 {
        let zoom = (self.pixels_per_unit / DEFAULT_PIXELS_PER_UNIT).max(1.0);
        HOMOTOPY_TRAVEL * zoom * self.route.sections() as f64
    }

    /// Moves p by `delta` screen points. p is not a point on screen, so this
    /// ignores the zoom.
    fn move_parameter(&mut self, delta: Vec2) {
        self.parameter += complex(delta) / DEFAULT_PIXELS_PER_UNIT;
    }

    /// Moves each grid edge whose key is held (W/A/S/D: top, left, bottom,
    /// right) by `delta` grid units. Returns whether any edge key was held.
    fn move_edges(&mut self, keys: &ButtonInput<KeyCode>, delta: DVec2) -> bool {
        let mut held = false;
        for (key, edge, step) in [
            (KeyCode::KeyA, &mut self.x_min, delta.x),
            (KeyCode::KeyD, &mut self.x_max, delta.x),
            (KeyCode::KeyW, &mut self.y_max, delta.y),
            (KeyCode::KeyS, &mut self.y_min, delta.y),
        ] {
            if keys.pressed(key) {
                *edge = (*edge + step).clamp(-DOMAIN_LIMIT, DOMAIN_LIMIT);
                held = true;
            }
        }
        held
    }
}

fn model(app: &App) -> Model {
    let window = app
        .new_window()
        .primary()
        .title("Conformal Map Visualizer");
    // On the web the page's CSS sizes the canvas to the viewport.
    #[cfg(not(target_arch = "wasm32"))]
    let window = window.size(1200, 800);
    let window = window
        .view(view)
        .key_pressed(key_pressed)
        .mouse_wheel(mouse_wheel)
        .touch(touch)
        .build();
    // Only redraw on input, unless something moves (see `set_update_mode`).
    app.set_update_mode(UpdateMode::wait());

    new_model(window, link::fragment())
}

/// The state at the start, showing the scene in `fragment`.
fn new_model(window: Entity, fragment: String) -> Model {
    let example = &EXAMPLES[0];
    let [x_min, x_max, y_min, y_max] = example.grid;
    let chain = expr::parse(example.formula).expect("examples parse").chain();
    let route = Route::all(chain.len() - 1, Path::Straight);
    let mut model = Model {
        window,
        example: 0,
        formula: example.formula.into(),
        formula_error: None,
        // GridShaderPlugin starts the shader with this example.
        chain,
        route,
        anchor: None,
        new_function: None,
        settling_until: 0.0,
        pixels_per_unit: DEFAULT_PIXELS_PER_UNIT,
        position: DVec2::ZERO,
        resolution: DEFAULT_RESOLUTION,
        x_min,
        x_max,
        y_min,
        y_max,
        parameter: example.parameter,
        homotopy: 1.0,
        morph: Morph::Off,
        coloring: Coloring::default(),
        // Due at once, so the URL shows the link in its own words.
        link_written: (fragment.clone(), f64::NEG_INFINITY),
        link_copied: None,
        continuous: false,
        wake_at: None,
        rest: Rest {
            at: Vec2::ZERO,
            since: Instant::now(),
            touch: false,
        },
        drag: None,
        space_tap: false,
        scrolled_border: None,
        shift_held: false,
        peek_button: false,
        peek: false,
        last_mouse: Vec2::ZERO,
        half_view: vec2(600.0, 400.0),
        touches: Vec::new(),
        panel: egui::Rect::NOTHING,
        panel_open: None,
        slider_held: false,
        patch_seed: None,
    };
    model.set_scene(Scene::from_fragment(&fragment));
    model
}

/// Plays the animation, keeps the page URL showing the scene, and redraws
/// every frame only while either needs it or a key is `steering` something.
/// Otherwise the app waits for input, or until `wake_at` if that comes first,
/// when the hover readout is due.
fn animate(app: &App, model: &mut Model, steering: bool, wake_at: Option<Instant>) {
    let now = app.time() as f64;
    match model.morph {
        Morph::Off => {}
        Morph::Starting(homotopy) => {
            model.morph = Morph::Since(now - morph::time_at(homotopy, model.route.sections()));
            model.homotopy = homotopy;
        }
        Morph::Since(start) => {
            model.homotopy = morph::blend(now - start, model.route.sections());
        }
    }

    let fragment = model.scene().to_fragment();
    let (written, written_at) = &model.link_written;
    let link_due = fragment != *written;
    if link_due && now - written_at >= LINK_INTERVAL {
        link::set_fragment(&fragment);
        model.link_written = (fragment, now);
    }

    let settling = now < model.settling_until;
    let continuous = model.morph != Morph::Off || link_due || steering || settling;
    let wake_at = wake_at.filter(|_| !continuous);
    // Only on a change: Bevy redraws once more whenever the mode changes.
    if continuous != model.continuous || wake_at != model.wake_at {
        model.continuous = continuous;
        model.wake_at = wake_at;
        app.set_update_mode(match wake_at {
            _ if continuous => UpdateMode::Continuous,
            Some(at) => {
                UpdateMode::reactive(at.saturating_duration_since(Instant::now()) + WAKE_SLACK)
            }
            None => UpdateMode::wait(),
        });
    }
}

fn update(app: &App, model: &mut Model) {
    let ctx = app.egui_for_window(model.window);
    let window = app.window_rect();
    model.half_view = 0.5 * vec2(window.w(), window.h());
    // They take effect from the next frame on.
    let math_fonts = has_math_fonts(&ctx);
    if !math_fonts {
        add_math_fonts(&ctx);
        ctx.request_repaint();
    }

    // Space plays and pauses (when let go, below), so it must not also click
    // the button or checkbox that has keyboard focus, as egui would.
    if !ctx.text_edit_focused() && ctx.input(|i| i.key_pressed(egui::Key::Space)) {
        ctx.memory_mut(|memory| {
            if let Some(id) = memory.focused() {
                memory.surrender_focus(id);
            }
        });
    }

    // Phones get the canvas first; the panel opens from its header.
    let open = model
        .panel_open
        .unwrap_or_else(|| ctx.content_rect().width() > 700.0);
    let gap = 10.0;
    // egui caps a window's content at the screen height, leaving out the frame
    // and the gap, so a long panel would lose its bottom instead of scrolling.
    let frame = egui::Frame::window(&ctx.global_style());
    let frame_height = frame.total_margin().sum().y;
    let was_held = std::mem::take(&mut model.slider_held);
    let fade = ctx.animate_bool_with_time(
        egui::Id::new("control window fade"),
        was_held,
        PANEL_FADE_SECONDS,
    );
    let opacity = egui::lerp(1.0..=HELD_PANEL_OPACITY, fade);
    let panel = egui::Window::new("Conformal Map")
        // egui remembers a window's size by id; closed it should shrink to the header.
        .id(egui::Id::new(("control window", open)))
        .title_bar(false)
        .frame(frame.multiply_with_opacity(opacity))
        .anchor(egui::Align2::LEFT_TOP, [gap, gap])
        .max_height(ctx.content_rect().height() - 2.0 * gap - frame_height)
        .resizable(false)
        .vscroll(open)
        .show(&ctx, |ui| {
            ui.multiply_opacity(opacity);
            if panel_header(ui, open).clicked() {
                model.panel_open = Some(!open);
            }
            if open {
                ui.separator();
                controls(ui, model);
            }
        });
    if let Some(panel) = panel {
        model.panel = panel.response.rect;
    }
    // The window is already drawn, so the fade starts or ends on the next frame.
    if model.slider_held != was_held {
        ctx.request_repaint();
    }
    if let Some(functions) = model.new_function.take() {
        model.settling_until = app.time() as f64 + SHADER_SETTLE;
        app.command_scope(|mut commands| {
            commands.queue(move |world: &mut World| set_grid_shader(world, &functions));
        });
    }

    let keys = app.keys();
    let typing = ctx.text_edit_focused();
    let over_ui = ctx.is_pointer_over_egui();
    model.peek = model.peek_button || (!typing && keys.pressed(KeyCode::KeyX));

    // Space alone plays or pauses when let go; scrolling or dragging with it
    // held uses it as a modifier instead. Watching the key itself, not its
    // events, lets Ctrl come and go around it.
    let shift = ctx.input(|i| i.modifiers.shift);
    if shift && !model.shift_held {
        model.scrolled_border = model.grab_for_scroll(app.mouse());
    } else if !shift {
        model.scrolled_border = None;
    }
    model.shift_held = shift;
    if keys.just_pressed(KeyCode::Space) {
        model.space_tap = !typing;
    }
    if keys.just_released(KeyCode::Space) && std::mem::take(&mut model.space_tap) && !typing {
        model.toggle_morph();
    }

    // ← / → steer the homotopy, unless an egui widget has the keyboard (a
    // focused slider takes the arrows itself).
    let mut steer = 0.0;
    if ctx.memory(|memory| memory.focused().is_none()) {
        for (key, direction) in [(KeyCode::ArrowLeft, -1.0), (KeyCode::ArrowRight, 1.0)] {
            if keys.pressed(key) {
                steer += direction;
            }
        }
    }
    if steer != 0.0 {
        let dt = app.time_delta().min(MAX_KEY_STEP);
        model.scrub(steer * dt * HOMOTOPY_TRAVEL as f32 / ARROW_SECONDS);
    }

    let mouse = app.mouse();
    let moved = mouse - model.last_mouse;
    model.last_mouse = mouse;
    if !typing && !over_ui && model.drag.is_none() && moved != Vec2::ZERO {
        key_gesture(model, &keys, moved, moved);
    }
    let modifiers = ctx.input(|i| i.modifiers);
    mouse_drag(app, model, &keys, modifiers, over_ui);
    let hover = hover(app, &ctx, model, &keys, over_ui);
    let wake_at = match hover {
        Hover::Due(at) => Some(at),
        _ => None,
    };
    animate(app, model, steer != 0.0, wake_at);
    if let Some(patch) = model.patch() {
        model.patch_seed = Some(patch.source);
    }
    if math_fonts {
        axis_labels(&ctx, model);
        if let Hover::Shown(cursor) = hover {
            show_readout(&ctx, model, cursor);
        }
    }
}

/// The hover readout: what landed under the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Hover {
    Hidden,
    /// The cursor is resting; the readout shows at this time.
    Due(Instant),
    /// For the cursor here, in canvas points.
    Shown(Vec2),
}

/// Keys that scrub, pan or move something while held: the readout waits
/// until they are let go.
const GESTURE_KEYS: [KeyCode; 9] = [
    KeyCode::Space,
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::KeyP,
    KeyCode::KeyF,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
];

/// The readout shows once the cursor has rested on the canvas, not over the
/// control window (`over_ui`), for `READOUT_DELAY` with no button, gesture
/// key or modifier down and nothing scrolled (see `mouse_wheel`).
fn hover(
    app: &App,
    ctx: &egui::Context,
    model: &mut Model,
    keys: &ButtonInput<KeyCode>,
    over_ui: bool,
) -> Hover {
    let now = Instant::now();
    // egui's idea of the cursor, which also knows when it has left the window.
    let Some(cursor) = pointer(ctx, model) else {
        // Coming back into the window counts as a move.
        model.rest.since = now;
        return Hover::Hidden;
    };
    let rest = &mut model.rest;
    if cursor != rest.at {
        rest.at = cursor;
        rest.since = now;
        // A finger may move the cursor too; see `touch`.
        if model.touches.is_empty() {
            rest.touch = false;
        }
    }
    let modifiers = ctx.input(|i| i.modifiers);
    let busy = app.mouse_buttons().get_pressed().next().is_some()
        || model.drag.is_some()
        || !model.touches.is_empty()
        || modifiers.shift
        || modifiers.ctrl
        || modifiers.alt
        || modifiers.command
        || (!ctx.text_edit_focused() && GESTURE_KEYS.iter().any(|&key| keys.pressed(key)));
    if busy {
        rest.since = now;
    }
    if over_ui || rest.touch {
        return Hover::Hidden;
    }
    let due = rest.since + READOUT_DELAY;
    if now + WAKE_SLACK >= due {
        Hover::Shown(cursor)
    } else {
        Hover::Due(due)
    }
}

/// Left-drag moves the grid border drawn nearest the cursor (with Ctrl too,
/// which keeps a laptop touchpad on), Shift + left-drag scrubs the homotopy,
/// Ctrl + Shift + left-drag sets the resolution. Space + left-drag and
/// middle-drag pan, right-drag moves the parameter p. Drags must start on
/// the canvas.
fn mouse_drag(
    app: &App,
    model: &mut Model,
    keys: &ButtonInput<KeyCode>,
    modifiers: egui::Modifiers,
    over_ui: bool,
) {
    let buttons = app.mouse_buttons();
    let mouse = app.mouse();
    for button in [MouseButton::Left, MouseButton::Middle, MouseButton::Right] {
        if !buttons.just_pressed(button) || over_ui {
            continue;
        }
        let drag = match button {
            MouseButton::Right => Some(Drag::Parameter),
            MouseButton::Middle => Some(Drag::Pan),
            // Space wins over the modifiers, as in `mouse_wheel`.
            _ if keys.pressed(KeyCode::Space) => {
                model.space_tap = false;
                Some(Drag::Pan)
            }
            _ if modifiers.shift && modifiers.ctrl => Some(Drag::Resolution),
            _ if modifiers.shift => {
                model.scrolled_border = model.grab_for_scroll(mouse);
                Some(Drag::HeldBorder)
            }
            _ if modifiers.alt => Some(Drag::Grid),
            _ => Some(Drag::Pan),
        };
        if let Some(drag) = drag {
            model.drag = Some((button, drag, mouse));
        }
    }
    let Some((button, drag, from)) = model.drag else {
        return;
    };
    if !buttons.pressed(button) {
        model.drag = None;
        return;
    }
    let delta = mouse - from;
    match drag {
        Drag::Pan => model.position += delta.as_dvec2() / model.pixels_per_unit,
        Drag::Parameter => model.move_parameter(delta * if modifiers.shift { FINE } else { 1.0 }),
        Drag::HeldBorder => model.drag_held_border(delta),
        Drag::Grid => model.move_grid(delta),
        Drag::Resolution => model.refine(delta.x + delta.y),
    }
    model.drag = Some((button, drag, mouse));
}

/// Hold a key and scroll or move the mouse: W/A/S/D move grid edges (two
/// opposite ones slide the grid), P moves the parameter, F blends the homotopy.
/// `delta` is in screen points with y up; `edge_delta` is the same motion as
/// the edges should take it (see [`along_held_edges`]). Returns whether a key
/// was held.
fn key_gesture(
    model: &mut Model,
    keys: &ButtonInput<KeyCode>,
    delta: Vec2,
    edge_delta: Vec2,
) -> bool {
    let mut used = model.move_edges(keys, edge_delta.as_dvec2() * model.edge_units_per_point());
    if keys.pressed(KeyCode::KeyP) {
        model.move_parameter(delta);
        used = true;
    }
    if keys.pressed(KeyCode::KeyF) {
        model.scrub(delta.x + delta.y);
        used = true;
    }
    used
}

/// The formula for f(z), and examples to start from.
fn function_editor(ui: &mut egui::Ui, model: &mut Model) {
    let example = &EXAMPLES[model.example];
    let edited = model.formula != example.formula;

    let mut picked = None;
    egui::ComboBox::from_label("Example")
        .selected_text(if edited { "Custom" } else { example.name })
        .show_ui(ui, |ui| {
            for (i, example) in EXAMPLES.iter().enumerate() {
                let current = i == model.example && !edited;
                if ui.selectable_label(current, example.name).clicked() {
                    picked = Some(i);
                }
            }
        });
    if edited && ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        picked = Some(model.example);
    }
    if let Some(index) = picked {
        model.set_example(index);
    }
    hint(ui, "next: E · picking one resets the grid, p and the view");

    ui.horizontal(|ui| {
        ui.monospace("f(z) =");
        let mut formula = model.formula.clone();
        let field = egui::TextEdit::singleline(&mut formula)
            .font(egui::TextStyle::Monospace)
            .desired_width(f32::INFINITY);
        if ui.add(field).changed() {
            model.set_formula(formula);
        }
    });
    match &model.formula_error {
        Some(error) => {
            ui.colored_label(ui.visuals().error_fg_color, error.to_string());
            hint(ui, "still showing the last formula that worked");
        }
        None if edited => hint(ui, "your own function · Esc goes back to the example"),
        None => {
            ui.label(example.about);
            for &(text, url) in example.links {
                ui.hyperlink_to(text, url);
            }
        }
    }
}

/// Which stages the homotopy runs over, and how. Each button shows the stage's
/// formula: phones have no number keys, and the keys stop at 9.
fn stage_picker(ui: &mut egui::Ui, model: &mut Model) {
    ui.horizontal(|ui| {
        ui.label("Stages");
        if ui.selectable_label(model.route.is_all(model.count()), "All").clicked() {
            model.select_all();
        }
    });
    let mut tapped = None;
    ui.horizontal_wrapped(|ui| {
        for stage in 1..=model.count() {
            let label = format!("{stage}: {}", model.chain[stage]);
            if ui.selectable_label(model.route.contains(stage), label).clicked() {
                tapped = Some(stage);
            }
        }
    });
    if let Some(stage) = tapped {
        if ui.input(|i| i.modifiers.shift) {
            model.extend_to(stage);
        } else {
            model.tap(stage);
        }
    }
    if ui.input(|i| i.has_touch_screen()) {
        hint(ui, "tap two stages for a range");
    } else {
        hint(ui, "1 – 9 picks, shift + 1 – 9 makes a range, 0 all");
        hint(ui, ", . move it · shift + , . move its end");
    }
    let (start, end) = (model.route.start(), model.route.last);
    hint(ui, &format!("from {} to {}", model.chain[start], model.chain[end]));

    ui.add_enabled_ui(end - start > 1, |ui| {
        ui.horizontal_wrapped(|ui| {
            for path in Path::ALL {
                if ui.selectable_label(model.route.path == path, path.label()).clicked() {
                    model.set_path(path);
                }
            }
        });
    });
    if end - start > 1 {
        hint(ui, "T switches");
    }
}

/// The homotopy as a line to drag along, with a mark at each stage it shows.
fn homotopy_bar(ui: &mut egui::Ui, model: &mut Model) {
    const INSET: f32 = 8.0;
    /// More marks than this have no room for their numbers.
    const MOST_LABELS: usize = 16;
    let size = egui::vec2(240.0, 34.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
    let x_range = rect.left() + INSET..=rect.right() - INSET;
    if response.is_pointer_button_down_on()
        && let Some(pos) = response.interact_pointer_pos()
    {
        model.homotopy = egui::remap_clamp(pos.x, x_range.clone(), 0.0..=1.0) as f64;
        model.morph = Morph::Off;
    }
    model.slider_held |= response.dragged();

    let painter = ui.painter();
    let visuals = ui.style().interact(&response);
    let (rail, marks) = (ui.visuals().weak_text_color(), ui.visuals().text_color());
    let y = rect.top() + 10.0;
    let x = |t: f64| egui::lerp(x_range.clone(), t as f32);
    let line = |to: f64, color| {
        let ends = [egui::pos2(x(0.0), y), egui::pos2(x(to), y)];
        painter.line_segment(ends, egui::Stroke::new(3.0_f32, color));
    };
    line(1.0, rail);
    line(model.homotopy, ui.visuals().selection.bg_fill);

    let (start, end) = (model.route.start(), model.route.last);
    let font = egui::FontId::proportional(10.0);
    for stage in start..=end {
        let Some(t) = model.route.position_of(stage) else {
            continue;
        };
        painter.circle_filled(egui::pos2(x(t), y), 3.0, marks);
        if end - start <= MOST_LABELS {
            let at = egui::pos2(x(t), y + 7.0);
            painter.text(at, egui::Align2::CENTER_TOP, stage.to_string(), font.clone(), marks);
        }
    }
    painter.circle_filled(egui::pos2(x(model.homotopy), y), 7.0, visuals.fg_stroke.color);
}

/// The title of the control window, with an arrow. Tapping anywhere on it
/// opens or closes the window: egui's own collapse arrow is too small a
/// target on a phone.
fn panel_header(ui: &mut egui::Ui, open: bool) -> egui::Response {
    let size = egui::vec2(240.0, 36.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
    let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
    let color = ui.style().interact(&response).text_color();
    let painter = ui.painter();

    let center = egui::pos2(rect.left() + 12.0, rect.center().y);
    let s = 6.0;
    let corners = if open {
        [(-s, -0.6 * s), (s, -0.6 * s), (0.0, 0.8 * s)]
    } else {
        [(-0.6 * s, -s), (-0.6 * s, s), (0.8 * s, 0.0)]
    };
    let corners = corners.map(|(x, y)| center + egui::vec2(x, y)).to_vec();
    painter.add(egui::Shape::convex_polygon(corners, color, egui::Stroke::NONE));
    painter.text(
        egui::pos2(rect.left() + 28.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "Conformal Map",
        egui::FontId::proportional(18.0),
        color,
    );
    response
}

/// Copies a link that opens exactly this view. The page URL always shows it
/// too, so copying from the address bar works as well.
fn share_button(ui: &mut egui::Ui, model: &mut Model) {
    let fragment = model.scene().to_fragment();
    ui.horizontal(|ui| {
        if ui.button("Copy link").clicked() {
            ui.ctx().copy_text(link::url(&fragment));
            model.link_copied = Some(fragment.clone());
        }
        if model.link_copied.as_ref() == Some(&fragment) {
            ui.label("copied");
        } else {
            hint(ui, "to share exactly this view");
        }
    });
}

/// A faint line under a control that lists its shortcuts.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).weak());
}

fn controls(ui: &mut egui::Ui, model: &mut Model) {
    share_button(ui, model);
    ui.separator();
    function_editor(ui, model);
    ui.separator();
    stage_picker(ui, model);
    ui.separator();

    model.peek_button = ui
        .button("Hold to show source")
        .is_pointer_button_down_on();
    hint(ui, "peek at the source grid: hold the button or X");
    ui.horizontal(|ui| {
        ui.label("Homotopy");
        let mut playing = model.morph != Morph::Off;
        if ui.checkbox(&mut playing, "Animate").changed() {
            model.toggle_morph();
        }
    });
    homotopy_bar(ui, model);
    hint(ui, "play / pause: space or M");
    // Spelled out: the default egui fonts have no ← / → glyphs.
    hint(ui, "scrub: left / right arrow, space + scroll,");
    hint(ui, "or hold F + scroll or move the mouse");

    ui.separator();
    // Fingers pinch to zoom.
    let touch = ui.input(|i| i.has_touch_screen());
    ui.horizontal(|ui| {
        if !touch {
            let speed = model.pixels_per_unit * ZOOM_PER_POINT;
            let zoom = ui.add(
                egui::DragValue::new(&mut model.pixels_per_unit)
                    .speed(speed)
                    .range(SCALE_RANGE)
                    .prefix("Zoom ")
                    .custom_formatter(|zoom, _| zoom_text(zoom)),
            );
            model.slider_held |= zoom.dragged();
        }
        if ui.button("Reset view").clicked() {
            model.reset_view();
        }
    });
    if !touch {
        hint(ui, "zoom: ctrl + scroll (around the cursor)");
    }
    hint(ui, "pan: drag or scroll · reset view: H");
    hint(ui, "rest the cursor to see what landed under it");

    ui.separator();
    let resolution =
        ui.add(egui::Slider::new(&mut model.resolution, 0.0..=MAX_UI_RESOLUTION).text("Resolution"));
    model.slider_held |= resolution.dragged();
    hint(ui, "finer / coarser: ctrl + shift + scroll or drag");
    ui.label("Grid domain");
    egui::Grid::new("grid domain").show(ui, |ui| {
        for (axis, min, max) in [
            ("x", &mut model.x_min, &mut model.x_max),
            ("y", &mut model.y_min, &mut model.y_max),
        ] {
            ui.label(axis);
            model.slider_held |= edge_field(ui, min).dragged();
            ui.label("to");
            model.slider_held |= edge_field(ui, max).dragged();
            ui.end_row();
        }
    });
    hint(ui, "drag a number, or click it to type one");
    hint(ui, "shift + scroll or drag across a border to move it");
    hint(ui, "(the one nearest the cursor) · alt + drag or scroll:");
    hint(ui, "move the source grid");
    hint(ui, "hold W / A / S / D (top / left / bottom / right edge)");
    hint(ui, "+ scroll or move the mouse to drag that edge;");
    hint(ui, "A + D or W + S slide the grid, all four move it");
    hint(ui, "touchpad off while a key is held? press Ctrl first:");
    hint(ui, "Ctrl + A + scroll and Ctrl + Space work the same");

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Color");
        for coloring in Coloring::ALL {
            ui.selectable_value(&mut model.coloring, coloring, coloring.label());
        }
    });
    hint(ui, "C cycles");
    legend(ui, model.coloring, &model.stage_name(model.route.last));

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Parameter p");
        if ui.button("Reset").clicked() {
            model.reset_parameter();
        }
    });
    ui.horizontal(|ui| {
        let re = ui.add(
            egui::DragValue::new(&mut model.parameter.re)
                .speed(0.01)
                .prefix("re "),
        );
        let im = ui.add(
            egui::DragValue::new(&mut model.parameter.im)
                .speed(0.01)
                .prefix("im "),
        );
        model.slider_held |= re.dragged() || im.dragged();
    });
    model.slider_held |= xy_pad(ui, &mut model.parameter, 3.0).dragged();
    hint(ui, "drag the pad or right-drag the canvas, shift: 5× finer");
    hint(ui, "reset: R");
    hint(ui, "hold P + scroll or move the mouse");

    ui.separator();
    // No fps readout: the app only redraws on input, so it would count events.
    hint(
        ui,
        &format!(
            "{} segments mapped on the GPU",
            model.grid_layout().segment_count()
        ),
    );
}

/// Explains the current color scheme, using the same palettes as
/// `src/grid.wgsl`. `f` names the function it colors by.
fn legend(ui: &mut egui::Ui, coloring: Coloring, f: &str) {
    match coloring {
        Coloring::White => {}
        Coloring::Source => {
            gradient(ui, |t| source(true, t), ["horizontal: bottom", "", "top"]);
            gradient(ui, |t| source(false, t), ["vertical: left", "", "right"]);
            ui.label("Follow a line from the grid to its image.");
        }
        Coloring::Phase => {
            gradient(ui, |t| cyclic(t - 0.5), ["−180°", "0°", "+180°"]);
            ui.label("arg z of the source point: each line keeps the color it had before the map.");
        }
        Coloring::Stretch => {
            let octaves = 2.0f32.powf(STRETCH_OCTAVES);
            gradient(
                ui,
                |t| diverging(2.0 * t - 1.0),
                [&format!("÷{octaves}"), "×1", &format!("×{octaves}")],
            );
            ui.label(format!("|{f}'(z)|: how much {f} scales lengths near z."));
        }
        Coloring::Rotation => {
            gradient(ui, |t| cyclic(t - 0.5), ["−180°", "0°", "+180°"]);
            ui.label(format!("arg {f}'(z): how far {f} turns directions near z."));
        }
    }
}

/// A color bar with labels under its left end, middle and right end.
fn gradient(ui: &mut egui::Ui, color: impl Fn(f32) -> [f32; 3], labels: [&str; 3]) {
    const STEPS: usize = 64;
    let width = ui.available_width().min(200.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
    let painter = ui.painter();
    let x = |t: f32| egui::lerp(rect.left()..=rect.right(), t);
    for i in 0..STEPS {
        let (t0, t1) = (i as f32 / STEPS as f32, (i + 1) as f32 / STEPS as f32);
        let [r, g, b] = color((t0 + t1) / 2.0).map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8);
        painter.rect_filled(
            egui::Rect::from_x_y_ranges(x(t0)..=x(t1), rect.top()..=rect.top() + 12.0),
            0.0,
            egui::Color32::from_rgb(r, g, b),
        );
    }
    let font = egui::FontId::proportional(11.0);
    let text_color = ui.visuals().text_color();
    for (label, t, align) in [
        (labels[0], 0.0, egui::Align2::LEFT_TOP),
        (labels[1], 0.5, egui::Align2::CENTER_TOP),
        (labels[2], 1.0, egui::Align2::RIGHT_TOP),
    ] {
        let pos = egui::pos2(x(t), rect.top() + 14.0);
        painter.text(pos, align, label, font.clone(), text_color);
    }
}

// Palettes in sRGB, mirroring the ones in `src/grid.wgsl`.

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

fn cyclic(t: f32) -> [f32; 3] {
    [0.0, 1.0 / 3.0, 2.0 / 3.0].map(|offset| 0.5 + 0.5 * (TAU * (t + offset)).cos())
}

fn diverging(t: f32) -> [f32; 3] {
    let neutral = [0.85; 3];
    if t < 0.0 {
        mix(neutral, [0.2, 0.5, 1.0], -t)
    } else {
        mix(neutral, [1.0, 0.45, 0.1], t)
    }
}

fn source(horizontal: bool, s: f32) -> [f32; 3] {
    if horizontal {
        mix([1.0, 0.3, 0.2], [1.0, 0.9, 0.3], s)
    } else {
        mix([0.2, 0.8, 1.0], [0.7, 0.4, 1.0], s)
    }
}

fn key_pressed(app: &App, model: &mut Model, key: KeyCode) {
    if app
        .egui_for_window(model.window)
        .text_edit_focused()
    {
        return;
    }
    let shift = app.egui_for_window(model.window).input(|i| i.modifiers.shift);
    const STAGE_KEYS: [KeyCode; 10] = [
        KeyCode::Digit0,
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    match key {
        KeyCode::KeyR => model.reset_parameter(),
        KeyCode::KeyE => model.set_example((model.example + 1) % EXAMPLES.len()),
        KeyCode::KeyH | KeyCode::Home => model.reset_view(),
        KeyCode::KeyC => model.coloring = model.coloring.next(),
        KeyCode::KeyM => model.toggle_morph(),
        KeyCode::KeyT => model.set_path(model.route.path.next()),
        KeyCode::Comma => model.move_selection(-1, shift),
        KeyCode::Period => model.move_selection(1, shift),
        _ => match STAGE_KEYS.iter().position(|&k| k == key) {
            Some(0) => model.select_all(),
            Some(stage) if shift => model.extend_to(stage),
            Some(stage) => model.pick(stage),
            None => {}
        },
    }
}

/// One finger pans, two fingers pinch to zoom. Touches that start on the
/// control window are left to egui.
fn touch(app: &App, model: &mut Model, touch: TouchInput) {
    // nannou already hands over canvas points (centred, y up), like `app.mouse()`.
    let pos = touch.position;
    let ctx = app.egui_for_window(model.window);
    // No readout until the mouse moves: a finger may have moved the cursor.
    model.rest = Rest {
        at: pointer(&ctx, model).unwrap_or(model.rest.at),
        since: Instant::now(),
        touch: true,
    };
    let on_panel = model
        .panel
        .contains(canvas_to_egui(pos, app.main_window().size_points()));
    let index = model.touches.iter().position(|&(id, _)| id == touch.id);
    match (touch.phase, index) {
        (TouchPhase::Started, None) => {
            if !on_panel && !ctx.egui_wants_pointer_input() {
                model.touches.push((touch.id, pos));
            }
        }
        (TouchPhase::Moved, Some(i)) => {
            let previous = model.touches[i].1;
            // egui owns the gesture once it is dragging one of its widgets.
            let egui_busy = ctx.egui_is_using_pointer();
            match model.touches.as_slice() {
                _ if egui_busy => {}
                [_] => model.position += (pos - previous).as_dvec2() / model.pixels_per_unit,
                [a, b] => {
                    let other = if i == 0 { b.1 } else { a.1 };
                    // The midpoint between the fingers moves by half of this finger's motion.
                    model.position += (pos - previous).as_dvec2() / 2.0 / model.pixels_per_unit;
                    let before = previous.distance(other);
                    if before > 1.0 {
                        model.zoom((pos.distance(other) / before) as f64, (pos + other) / 2.0);
                    }
                }
                _ => {}
            }
            model.touches[i].1 = pos;
        }
        (TouchPhase::Ended | TouchPhase::Canceled, Some(i)) => {
            model.touches.remove(i);
        }
        _ => {}
    }
}

/// Converts canvas points (centred, y up) in a window of `size` points to
/// egui's screen points (top left, y down).
fn canvas_to_egui(pos: Vec2, size: Vec2) -> egui::Pos2 {
    egui::pos2(pos.x + size.x / 2.0, size.y / 2.0 - pos.y)
}

/// The inverse of `canvas_to_egui`.
fn egui_to_canvas(pos: egui::Pos2, size: Vec2) -> Vec2 {
    vec2(pos.x - size.x / 2.0, size.y / 2.0 - pos.y)
}

/// Where egui sees the cursor, in canvas points; `None` outside the window.
fn pointer(ctx: &egui::Context, model: &Model) -> Option<Vec2> {
    let pos = ctx.pointer_hover_pos()?;
    Some(egui_to_canvas(pos, 2.0 * model.half_view))
}

fn mouse_wheel(app: &App, model: &mut Model, wheel: MouseWheel) {
    // The readout hides and waits for the scrolling to stop.
    model.rest.since = Instant::now();
    let ctx = app.egui_for_window(model.window);
    if ctx.is_pointer_over_egui() || ctx.text_edit_focused() {
        return;
    }
    // Pressing a wheel can tilt it, and a tilted wheel repeats sideways scroll
    // events for as long as it is held, which would pan the view away.
    if app.mouse_buttons().pressed(MouseButton::Middle) {
        return;
    }
    let delta = match wheel.unit {
        MouseScrollUnit::Pixel => vec2(wheel.x, wheel.y),
        MouseScrollUnit::Line => vec2(wheel.x, wheel.y) * POINTS_PER_LINE,
    };
    // Some platforms turn shift+wheel into horizontal scrolling.
    let dominant = if delta.y.abs() >= delta.x.abs() {
        delta.y
    } else {
        delta.x
    };

    let keys = app.keys();
    // Modifiers by meaning, not position, so a remapped key (e.g. CapsLock as
    // Ctrl via xkb `ctrl:nocaps`) counts too.
    let modifiers = ctx.input(|i| i.modifiers);
    // Positive y moves content down; the plane has y pointing up.
    let content = vec2(delta.x, -delta.y);

    // Gesture keys and Space win over the modifiers so they combine with Ctrl.
    // That matters on laptops: libinput's disable-while-typing turns the
    // touchpad off while a letter or Space is held, unless a modifier was
    // pressed first.
    if key_gesture(model, &keys, content, along_held_edges(&keys, content)) {
        return;
    }
    if keys.pressed(KeyCode::Space) {
        model.space_tap = false;
        model.scrub(content.x + content.y);
        return;
    }
    if modifiers.ctrl && modifiers.shift {
        model.refine(-dominant);
    } else if modifiers.ctrl {
        model.zoom((dominant as f64 * ZOOM_PER_POINT).exp(), app.mouse());
    } else if modifiers.shift {
        model.scroll_held_border(app.mouse(), content, wheel.unit == MouseScrollUnit::Line);
    } else if modifiers.alt {
        model.move_grid(content);
    } else {
        model.position += content.as_dvec2() / model.pixels_per_unit;
    }
}

/// A scroll goes mostly one way, and a mouse wheel only vertically. When only
/// left/right (A/D) or only top/bottom (W/S) edge keys are held, whichever way
/// the scroll goes drives that axis, so plain wheel scrolling moves any edge.
fn along_held_edges(keys: &ButtonInput<KeyCode>, content: Vec2) -> Vec2 {
    let any = |a, b| keys.pressed(a) || keys.pressed(b);
    let dominant = if content.x.abs() >= content.y.abs() {
        content.x
    } else {
        content.y
    };
    match (any(KeyCode::KeyA, KeyCode::KeyD), any(KeyCode::KeyW, KeyCode::KeyS)) {
        (true, false) => vec2(dominant, content.y),
        (false, true) => vec2(content.x, dominant),
        _ => content,
    }
}

fn complex(v: Vec2) -> Complex<f64> {
    Complex::new(v.x as f64, v.y as f64)
}

/// The GPU has no f64, so what it is sent ends up in f32 here.
fn gpu(z: Complex<f64>) -> Vec2 {
    vec2(z.re as f32, z.im as f32)
}

fn to_screen(model: &Model, z: Complex<f64>) -> Vec2 {
    ((dvec2(z.re, z.im) + model.position) * model.pixels_per_unit).as_vec2()
}

fn view(app: &App, model: &Model) {
    let draw = app.draw();
    draw.background().srgb(0.02, 0.02, 0.02);
    plane_grid(&draw, model);

    let patch = model.patch();
    let layout = patch.map_or_else(|| model.grid_layout(), |patch| patch.layout);
    let domain = model.full_layout();
    let segments = layout.segment_count();
    if segments == 0 {
        return;
    }
    let [(stage_a, a), (stage_b, b), (stage_c, c)] = model.mix().0;
    let params = GridParams {
        parameter: gpu(model.parameter),
        offset: model.position.as_vec2(),
        grid_min: gpu(layout.min),
        grid_max: gpu(layout.max),
        spacing: layout.spacing as f32,
        h_lines: layout.h_lines,
        h_segments: layout.h_segments,
        v_segments: layout.v_segments,
        pixels_per_unit: model.pixels_per_unit as f32,
        peek: model.peek as u32,
        stages: uvec3(stage_a as u32, stage_b as u32, stage_c as u32),
        weights: vec3(a as f32, b as f32, c as f32),
        color_stage: model.route.last as u32,
        line_width: LINE_WIDTH,
        coloring: model.coloring as u32,
        stretch_octaves: STRETCH_OCTAVES,
        domain_min: gpu(domain.min),
        domain_max: gpu(domain.max),
        half_view: model.half_view,
        patched: patch.is_some() as u32,
        units_per_u: patch.map_or(0.0, |patch| patch.units_per_u as f32),
        patch_source: patch.map_or(Vec2::ZERO, |patch| gpu(patch.source)),
        lines_from: patch.map_or(gpu(layout.min), |patch| gpu(patch.lines_from)),
        cubic0: patch.map_or(Vec2::ZERO, |patch| gpu(patch.cubic[0])),
        cubic1: patch.map_or(Vec2::ZERO, |patch| gpu(patch.cubic[1])),
        cubic2: patch.map_or(Vec2::ZERO, |patch| gpu(patch.cubic[2])),
        cubic3: patch.map_or(Vec2::ZERO, |patch| gpu(patch.cubic[3])),
    };
    // One quad, drawn once per segment; the shader places its corners.
    let grid = draw.shader_model(GridShader { params });
    let quad = grid.mesh().indexed([Vec3::ZERO; 4], [0, 1, 2, 2, 1, 3]);
    grid.instanced().primitive(quad).range(0..segments);
}

/// The background grid of the plane, under the mapped grid: faint minor
/// lines, major lines at a 1·2·5 step about `MAJOR_SPACING` points apart, and
/// the axes. Its numbers are drawn by `axis_labels`.
fn plane_grid(draw: &Draw, model: &Model) {
    let ppu = model.pixels_per_unit;
    let major = Step::near(MAJOR_SPACING, ppu);
    let divisions = major.minor_divisions();
    let minor = major.value() / divisions as f64;
    let half = model.half_view;
    let lines = |offset: f64, half: f32| {
        let reach = half as f64 / ppu;
        Step::lines(minor, -reach - offset, reach - offset)
            .map(move |k| (k, ((k as f64 * minor + offset) * ppu) as f32))
    };
    let level_of = |k: i64| match k {
        0 => 2,
        _ if k % divisions == 0 => 1,
        _ => 0,
    };
    // Minor lines first and the axes last, so the stronger lines cross on top.
    for (level, gray) in [MINOR_GRAY, MAJOR_GRAY, AXIS_GRAY].into_iter().enumerate() {
        for (k, x) in lines(model.position.x, half.x) {
            if level_of(k) == level {
                let line = draw.line().start(vec2(x, -half.y)).end(vec2(x, half.y));
                line.weight(PLANE_LINE_WIDTH).srgb(gray, gray, gray);
            }
        }
        for (k, y) in lines(model.position.y, half.y) {
            if level_of(k) == level {
                let line = draw.line().start(vec2(-half.x, y)).end(vec2(half.x, y));
                line.weight(PLANE_LINE_WIDTH).srgb(gray, gray, gray);
            }
        }
    }
}

/// The math fonts are loaded; egui panics on text in a family it lacks.
fn has_math_fonts(ctx: &egui::Context) -> bool {
    let family = egui::FontFamily::Name(MATH_FONT.into());
    ctx.fonts(|fonts| fonts.definitions().families.contains_key(&family))
}

/// Adds KaTeX's fonts as two families of their own, upright and italic,
/// keeping egui's fonts (and so the control window) as they are.
fn add_math_fonts(ctx: &egui::Context) {
    let mut fonts = ctx.fonts(|fonts| fonts.definitions().clone());
    for (name, data) in [
        (
            "KaTeX_Main-Regular",
            &include_bytes!("../assets/fonts/KaTeX_Main-Regular.ttf")[..],
        ),
        (
            "KaTeX_Math-Italic",
            &include_bytes!("../assets/fonts/KaTeX_Math-Italic.ttf")[..],
        ),
    ] {
        let data = Arc::new(egui::FontData::from_static(data));
        fonts.font_data.insert(name.to_owned(), data);
    }
    // Glyphs KaTeX lacks, like ·, come from egui's own fonts.
    let fallback = fonts.families[&egui::FontFamily::Proportional].clone();
    for (family, names) in [
        (MATH_FONT, &["KaTeX_Main-Regular"][..]),
        (
            MATH_ITALIC_FONT,
            &["KaTeX_Math-Italic", "KaTeX_Main-Regular"],
        ),
    ] {
        let names = names.iter().map(|&name| name.to_owned());
        fonts.families.insert(
            egui::FontFamily::Name(family.into()),
            names.chain(fallback.iter().cloned()).collect(),
        );
    }
    ctx.set_fonts(fonts);
}

fn math_font(size: f32, italic: bool) -> egui::FontId {
    let family = if italic { MATH_ITALIC_FONT } else { MATH_FONT };
    egui::FontId::new(size, egui::FontFamily::Name(family.into()))
}

/// Where the plane point `z` is on screen, in egui's points.
fn plane_to_egui(model: &Model, z: Complex<f64>) -> egui::Pos2 {
    canvas_to_egui(to_screen(model, z), 2.0 * model.half_view)
}

/// Numbers on the axes of the background grid, at its major lines: real
/// values on the real axis, multiples of i on the imaginary one, 0 once at
/// the origin. An axis off screen keeps its numbers at that edge of the
/// window. Numbers that would land on the control window or on each other
/// are left out.
fn axis_labels(ctx: &egui::Context, model: &Model) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let ppu = model.pixels_per_unit;
    let step = Step::near(MAJOR_SPACING, ppu);
    let size = 2.0 * model.half_view;
    let (position, half) = (model.position, model.half_view);
    let x_at = |x: f64| ((x + position.x) * ppu) as f32 + half.x;
    let y_at = |y: f64| half.y - ((y + position.y) * ppu) as f32;
    let visible = |offset: f64, half: f32| {
        let reach = half as f64 / ppu;
        Step::lines(step.value(), -reach - offset, reach - offset)
    };
    let (axis_x, axis_y) = (x_at(0.0), y_at(0.0));
    let real_pinned = !(0.0..=size.y).contains(&axis_y);
    let imaginary_pinned = !(0.0..=size.x).contains(&axis_x);

    let window = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(size.x, size.y));
    let mut taken = vec![model.panel];
    let mut label = |text: String, place: &dyn Fn(egui::Vec2) -> egui::Pos2| {
        let mut math = MathText::new(LABEL_SIZE);
        math.math(&text, LABEL_COLOR);
        let galley = painter.layout_job(math.job);
        let rect = egui::Rect::from_min_size(place(galley.size()), galley.size());
        if window.contains_rect(rect) && taken.iter().all(|other| !other.intersects(rect)) {
            painter.galley(rect.min, galley, LABEL_COLOR);
            taken.push(rect);
        }
    };
    // Below the real axis, centred; left of the imaginary one. Kept inside
    // the window, which pins them to its edge when their axis is off screen.
    let below = |x: f32| {
        move |label: egui::Vec2| {
            let bottom = size.y - LABEL_MARGIN - label.y;
            let top = (axis_y + LABEL_GAP).min(bottom).max(LABEL_MARGIN);
            egui::pos2(x - label.x / 2.0, top)
        }
    };
    let left_of = |y: f32| {
        move |label: egui::Vec2| {
            let right = size.x - LABEL_MARGIN - label.x;
            let left = (axis_x - LABEL_GAP - label.x).min(right).max(LABEL_MARGIN);
            egui::pos2(left, y - label.y / 2.0)
        }
    };
    if !real_pinned && !imaginary_pinned {
        label("0".to_owned(), &|label| {
            egui::pos2(axis_x - LABEL_GAP - label.x, axis_y + LABEL_GAP)
        });
    }
    // 0 on an axis only when it is pinned: there it is a coordinate, not the origin.
    for k in visible(position.x, half.x) {
        if k != 0 || real_pinned {
            label(step.real_label(k), &below(x_at(k as f64 * step.value())));
        }
    }
    for k in visible(position.y, half.y) {
        if k != 0 || imaginary_pinned {
            let y = y_at(k as f64 * step.value());
            label(step.imaginary_label(k), &left_of(y));
        }
    }
}

/// Text set like math, in the KaTeX fonts: letters (variables, and i) in
/// italics, numbers and signs upright, and words upright.
struct MathText {
    job: egui::text::LayoutJob,
    size: f32,
}

impl MathText {
    fn new(size: f32) -> Self {
        let job = egui::text::LayoutJob::default();
        MathText { job, size }
    }

    fn append(&mut self, text: &str, italic: bool, color: egui::Color32) {
        let format = egui::TextFormat::simple(math_font(self.size, italic), color);
        self.job.append(text, 0.0, format);
    }

    /// `text` as math: each run of letters in italics.
    fn math(&mut self, text: &str, color: egui::Color32) {
        let mut start = 0;
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        for (n, &(i, c)) in chars.iter().enumerate() {
            let italic = c.is_ascii_alphabetic();
            let ends = chars
                .get(n + 1)
                .is_none_or(|&(_, next)| next.is_ascii_alphabetic() != italic);
            if ends {
                self.append(&text[start..i + c.len_utf8()], italic, color);
                start = i + c.len_utf8();
            }
        }
    }

    fn words(&mut self, text: &str, color: egui::Color32) {
        self.append(text, false, color);
    }
}

/// The point w under the cursor (`cursor`, canvas points) and the points z
/// of the grid that landed there, with how much f stretches and turns near
/// each: a card by the cursor, and dots at w and at each z, joined by lines.
fn show_readout(ctx: &egui::Context, model: &Model, cursor: Vec2) {
    let ppu = model.pixels_per_unit;
    let w = complex(cursor) / ppu - Complex::new(model.position.x, model.position.y);
    let unmapped = model.unmapped();
    let (min, max) = model.domain();
    let (sources, count) = if unmapped {
        let inside = (min.re..=max.re).contains(&w.re) && (min.im..=max.im).contains(&w.im);
        (if inside { vec![w] } else { Vec::new() }, inside as usize)
    } else {
        // Not finer than f64 can tell apart near w.
        let tolerance = (READOUT_TOLERANCE / ppu).max(8.0 * f64::EPSILON * w.norm().max(1.0));
        let map = |z| model.map(z);
        let derivative = |z| model.map_derivative(z);
        readout::preimages(map, derivative, (min, max), w, tolerance, READOUT_SOURCES)
    };

    let visuals = &ctx.global_style().visuals;
    let (strong, weak) = (visuals.text_color(), visuals.weak_text_color());
    let mut text = MathText::new(READOUT_SIZE);
    let magnitude = |z: Complex<f64>| z.re.abs().max(z.im.abs());
    text.math(
        &if unmapped {
            "z = ".to_owned()
        } else if let Some(stage) = model.shown_stage() {
            format!("{}(z) = ", model.stage_name(stage))
        } else {
            "w = ".to_owned()
        },
        strong,
    );
    let decimals = readout::decimals(ppu, magnitude(w));
    text.math(&readout::complex(w, decimals), strong);
    if !unmapped && model.shown_stage().is_none() {
        text.words("   at ", weak);
        text.math(&format!("t = {:.2}", model.homotopy), weak);
    }
    for &z in &sources {
        if !unmapped {
            // As finely as one screen point at w tells apart near z.
            let points_per_unit = ppu * model.map_derivative(z).norm();
            let decimals = readout::decimals(points_per_unit, magnitude(z));
            text.math(&format!("\nz = {}", readout::complex(z, decimals)), strong);
        }
        let df = model.chain[model.route.last].derivative(z, model.parameter);
        if df.re.is_finite() && df.im.is_finite() {
            text.words("\n    stretch ", weak);
            text.math(&format!("×{}", readout::significant(df.norm())), weak);
            text.words(", turn ", weak);
            text.math(&readout::degrees(df.arg()), weak);
        }
    }
    if sources.is_empty() {
        text.words("\nnothing from the grid lands here", weak);
    } else if count > sources.len() {
        text.words(&format!("\nand {} more", count - sources.len()), weak);
    }

    let painter = ctx.layer_painter(egui::LayerId::background());
    let at = canvas_to_egui(cursor, 2.0 * model.half_view);
    if !unmapped {
        for &z in &sources {
            let source = plane_to_egui(model, z);
            painter.line_segment([source, at], egui::Stroke::new(LINK_WIDTH, LINK_COLOR));
            painter.circle_filled(source, SOURCE_MARK_RADIUS, MARK_COLOR);
        }
    }
    painter.circle_filled(at, CURSOR_MARK_RADIUS, MARK_COLOR);

    // To the side of the cursor away from the nearer window edges: left of
    // it in the right half, below it in the top half. The pivot is the
    // card's corner nearest the cursor.
    let (left, below) = (cursor.x > 0.0, cursor.y > 0.0);
    let pivot = match (left, below) {
        (true, true) => egui::Align2::RIGHT_TOP,
        (true, false) => egui::Align2::RIGHT_BOTTOM,
        (false, true) => egui::Align2::LEFT_TOP,
        (false, false) => egui::Align2::LEFT_BOTTOM,
    };
    let toward = egui::vec2(
        if left { -1.0 } else { 1.0 },
        if below { 1.0 } else { -1.0 },
    );
    let offset = READOUT_OFFSET * toward;
    egui::Area::new(egui::Id::new("hover readout"))
        .order(egui::Order::Tooltip)
        .interactable(false)
        .constrain(true)
        .pivot(pivot)
        .fixed_pos(at + offset)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.add(egui::Label::new(text.job).wrap_mode(egui::TextWrapMode::Extend));
            });
        });
}

/// The zoom for its field, which spans too many decades for plain digits.
fn zoom_text(zoom: f64) -> String {
    if (0.01..1e6).contains(&zoom) {
        format!("{zoom:.2}")
    } else {
        format!("{zoom:.3e}")
    }
}

/// A field for a grid edge. The edges have no useful bounds, so a slider has
/// no track to offer: this drags at a speed that follows the value, which
/// reaches any size quickly and stays fine near zero, and takes typed values.
fn edge_field(ui: &mut egui::Ui, edge: &mut f64) -> egui::Response {
    let speed = EDGE_DRAG_SPEED * edge.abs().max(1.0);
    ui.add(
        egui::DragValue::new(edge)
            .speed(speed)
            .range(-DOMAIN_LIMIT..=DOMAIN_LIMIT)
            .max_decimals(4),
    )
}

/// A square pad for dragging a complex value within `[-range, range]²`.
fn xy_pad(ui: &mut egui::Ui, value: &mut Complex<f64>, range: f64) -> egui::Response {
    let size = ui.available_width().min(200.0);
    let (response, painter) =
        ui.allocate_painter(egui::Vec2::splat(size), egui::Sense::click_and_drag());
    let rect = response.rect;
    let visuals = ui.style().interact(&response);

    // Pressing puts the value under the pointer; dragging then moves it by
    // the pointer's motion, so that with Shift (finer) it can lag behind
    // without jumping back when Shift comes up.
    let shift = ui.input(|i| i.modifiers.shift);
    let pressed = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.any_pressed());
    if let Some(pos) = response.interact_pointer_pos().filter(|_| pressed && !shift) {
        let t = (pos - rect.min) / rect.size();
        value.re = egui::lerp(-range..=range, t.x.clamp(0.0, 1.0) as f64);
        value.im = egui::lerp(range..=-range, t.y.clamp(0.0, 1.0) as f64);
    } else if response.dragged() {
        let speed = if shift { FINE } else { 1.0 };
        let delta = response.drag_delta() * speed / rect.width();
        *value += 2.0 * range * Complex::new(delta.x as f64, -delta.y as f64);
    }

    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let faint = egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    painter.line_segment([rect.center_top(), rect.center_bottom()], faint);
    painter.line_segment([rect.left_center(), rect.right_center()], faint);

    let t = egui::vec2(
        egui::remap_clamp(value.re, -range..=range, 0.0..=1.0) as f32,
        egui::remap_clamp(value.im, range..=-range, 0.0..=1.0) as f32,
    );
    painter.circle_filled(rect.min + t * rect.size(), 6.0, visuals.fg_stroke.color);
    painter.text(
        rect.left_top() + egui::vec2(4.0, 4.0),
        egui::Align2::LEFT_TOP,
        format!("{:.2} {:+.2}i", value.re, value.im),
        egui::FontId::monospace(11.0),
        ui.visuals().text_color(),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_compile() {
        for example in &EXAMPLES {
            let body =
                expr::to_wgsl(example.formula).unwrap_or_else(|e| panic!("{}: {e}", example.name));
            let source = format!("{COMPLEX_WGSL}\n{}", expr::function_wgsl(&body));
            let module = naga::front::wgsl::parse_str(&source)
                .unwrap_or_else(|e| panic!("{}: {}", example.name, e.emit_to_string(&source)));
            naga::valid::Validator::new(Default::default(), Default::default())
                .validate(&module)
                .unwrap_or_else(|e| panic!("{}: {e:?}", example.name));
        }
    }

    /// Compiles the whole grid shader, not just `f`, so mistakes in
    /// `src/grid.wgsl` show up here instead of only when the app starts.
    /// Bevy's import and bind group placeholder are swapped for plain WGSL.
    fn validate_grid_shader(chain: &[Function]) {
        let source = grid_shader_source(&expr::chain_wgsl(chain))
            .replace(
                "#import bevy_pbr::mesh_view_bindings::view",
                "struct View { clip_from_world: mat4x4<f32> };\n\
                 @group(0) @binding(0) var<uniform> view: View;",
            )
            .replace("#{MATERIAL_BIND_GROUP}", "2");
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(Default::default(), Default::default())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{e:?}"));
    }

    #[test]
    fn grid_shader_compiles_for_every_example() {
        let formulas = EXAMPLES.iter().map(|example| example.formula);
        for formula in formulas.chain(["sin(cos(p*z))", "exp(z + 1/z)", "p", "z"]) {
            validate_grid_shader(&expr::parse(formula).unwrap().chain());
        }
    }

    /// The app as `fragment` opens it, without its window.
    fn opened(fragment: &str) -> Model {
        new_model(Entity::PLACEHOLDER, fragment.to_owned())
    }

    /// The app showing a composition, paused.
    fn composition() -> Model {
        opened("f=sin(cos(p*z))&p=0.5,0.25&t=1")
    }

    fn selected(model: &Model) -> (usize, usize) {
        (model.route.first, model.route.last)
    }

    #[test]
    fn all_the_stages_make_the_whole_homotopy() {
        let model = composition();
        assert_eq!(model.count(), 3);
        assert_eq!(selected(&model), (1, 3));
        assert_eq!(model.route.start(), 0);
        assert_eq!(model.chain[model.route.last].to_string(), "sin(cos(p·z))");
    }

    #[test]
    fn picking_a_stage_selects_only_it() {
        let mut model = composition();
        model.pick(2);
        assert_eq!(selected(&model), (2, 2));
        assert_eq!(model.chain[model.route.start()].to_string(), "p·z");
        assert_eq!(model.chain[model.route.last].to_string(), "cos(p·z)");
        model.select_all();
        assert_eq!(selected(&model), (1, 3));
    }

    #[test]
    fn a_stage_that_does_not_exist_is_ignored() {
        let mut model = composition();
        model.pick(2);
        model.homotopy = 0.5;
        model.pick(9);
        model.extend_to(9);
        assert_eq!((selected(&model), model.homotopy), ((2, 2), 0.5));
    }

    #[test]
    fn shift_makes_a_range_from_the_stage_picked() {
        let mut model = composition();
        model.pick(3);
        model.extend_to(2);
        assert_eq!(selected(&model), (2, 3));
        // Still from the same stage, whichever way it goes.
        model.extend_to(3);
        assert_eq!(selected(&model), (3, 3));
    }

    #[test]
    fn two_taps_make_a_range() {
        let mut model = composition();
        model.tap(1);
        assert_eq!(selected(&model), (1, 1));
        model.tap(2);
        assert_eq!(selected(&model), (1, 2));
        // The third tap starts over.
        model.tap(3);
        assert_eq!(selected(&model), (3, 3));
        // The same stage twice stays one stage, and the next tap starts over.
        model.tap(3);
        assert_eq!(selected(&model), (3, 3));
        model.tap(1);
        assert_eq!(selected(&model), (1, 1));
    }

    #[test]
    fn comma_and_period_move_the_selection_and_stay_on_the_stages() {
        let mut model = composition();
        model.select(1, 2);
        model.move_selection(1, false);
        assert_eq!(selected(&model), (2, 3));
        model.move_selection(1, false);
        assert_eq!(selected(&model), (2, 3));
        model.move_selection(-1, false);
        model.move_selection(-1, false);
        assert_eq!(selected(&model), (1, 2));
    }

    #[test]
    fn shift_comma_and_period_move_only_the_end() {
        let mut model = composition();
        model.pick(2);
        model.move_selection(1, true);
        assert_eq!(selected(&model), (2, 3));
        model.move_selection(1, true);
        assert_eq!(selected(&model), (2, 3));
        model.move_selection(-1, true);
        model.move_selection(-1, true);
        assert_eq!(selected(&model), (2, 2));
    }

    #[test]
    fn going_on_to_the_next_stage_does_not_jump() {
        let mut model = composition();
        let z = Complex::new(0.3, 0.7);
        model.pick(2);
        model.homotopy = 1.0;
        let finished = model.map(z);
        model.pick(3);
        assert_eq!(model.homotopy, 0.0);
        assert!((model.map(z) - finished).norm() < 1e-12);
    }

    #[test]
    fn going_back_a_stage_does_not_jump() {
        let mut model = composition();
        let z = Complex::new(0.3, 0.7);
        model.pick(3);
        model.homotopy = 0.0;
        let started = model.map(z);
        model.pick(2);
        assert_eq!(model.homotopy, 1.0);
        assert!((model.map(z) - started).norm() < 1e-12);
    }

    #[test]
    fn a_selection_that_does_not_show_the_picture_shows_finished() {
        let mut model = composition();
        model.pick(1);
        model.homotopy = 0.25;
        model.pick(3);
        assert_eq!(model.homotopy, 1.0);
    }

    #[test]
    fn a_selection_changes_while_playing_from_its_start() {
        let mut model = composition();
        model.morph = Morph::Since(3.0);
        model.pick(2);
        assert_eq!((model.homotopy, model.morph), (0.0, Morph::Starting(0.0)));
    }

    #[test]
    fn selecting_stages_leaves_the_shader_alone() {
        let mut model = composition();
        model.new_function = None;
        model.pick(2);
        model.set_path(Path::Stages);
        model.select_all();
        assert_eq!(model.new_function, None);
    }

    #[test]
    fn through_the_stages_the_map_passes_each_of_them() {
        let mut model = composition();
        let (z, p) = (Complex::new(0.3, 0.7), model.parameter);
        model.set_path(Path::Stages);
        for (stage, t) in [(0, 0.0), (1, 1.0 / 3.0), (2, 2.0 / 3.0), (3, 1.0)] {
            model.homotopy = t;
            let exact = model.chain[stage].eval(z, p);
            assert!((model.map(z) - exact).norm() < 1e-12, "stage {stage}");
        }
    }

    #[test]
    fn overlapping_stages_keep_the_ends_of_the_selection_exact() {
        let mut model = composition();
        let (z, p) = (Complex::new(0.3, 0.7), model.parameter);
        model.set_path(Path::Overlap);
        model.homotopy = 1.0;
        assert_eq!(model.map(z), model.chain[3].eval(z, p));
        model.homotopy = 0.0;
        assert!(model.unmapped());
    }

    #[test]
    fn overlapping_stages_mix_three_functions() {
        let mut model = composition();
        let (z, p) = (Complex::new(0.3, 0.7), model.parameter);
        model.set_path(Path::Overlap);
        model.homotopy = model.route.position_of(2).unwrap();
        let mixed: Complex<f64> = model.mix().0.iter()
            .map(|&(stage, weight)| model.chain[stage].eval(z, p) * weight)
            .sum();
        assert_eq!(model.shown_stage(), None);
        assert!((model.map(z) - mixed).norm() < 1e-12);
        assert!(model.mix().0.iter().all(|part| part.1 > 0.0));
    }

    #[test]
    fn switching_paths_keeps_a_stage_on_screen() {
        let mut model = composition();
        model.homotopy = 1.0;
        model.set_path(Path::Stages);
        assert_eq!(model.homotopy, 1.0);
        model.homotopy = 0.0;
        model.set_path(Path::Straight);
        assert_eq!(model.homotopy, 0.0);
    }

    #[test]
    fn switching_paths_while_playing_goes_on_from_there() {
        let mut model = composition();
        model.homotopy = 0.25;
        model.morph = Morph::Since(3.0);
        model.set_path(Path::Stages);
        assert_eq!((model.homotopy, model.morph), (0.25, Morph::Starting(0.25)));
    }

    #[test]
    fn a_scroll_scrubs_a_third_as_fast_and_each_section_gets_the_whole_travel() {
        let mut model = composition();
        model.homotopy = 0.0;
        model.scrub(90.0);
        assert!((model.homotopy - 0.1).abs() < 1e-6, "{}", model.homotopy);
        model.homotopy = 0.0;
        model.set_path(Path::Stages);
        model.scrub(90.0);
        assert!((model.homotopy - 0.1 / 3.0).abs() < 1e-6, "{}", model.homotopy);
    }

    #[test]
    fn more_than_nine_stages_can_all_be_picked() {
        let mut model = opened("");
        model.set_formula(format!("{}z{}", "sin(".repeat(12), ")".repeat(12)));
        assert_eq!(model.count(), 12);
        model.tap(12);
        assert_eq!(selected(&model), (12, 12));
    }

    #[test]
    fn editing_the_formula_selects_all_its_stages() {
        let mut model = composition();
        model.pick(3);
        model.set_formula("z^2".into());
        assert_eq!(model.count(), 1);
        assert_eq!(selected(&model), (1, 1));
        assert_eq!(model.anchor, None);
        assert_eq!(model.chain[model.route.last].to_string(), "z^2");
    }

    #[test]
    fn a_formula_with_an_error_keeps_the_last_chain() {
        let mut model = composition();
        model.pick(3);
        model.new_function = None;
        model.set_formula("sin(".into());
        assert!(model.formula_error.is_some());
        assert_eq!(selected(&model), (1, 3));
        assert_eq!(model.chain[model.route.last].to_string(), "sin(cos(p·z))");
        assert_eq!(model.new_function, None);
    }

    #[test]
    fn picking_an_example_selects_all_its_stages() {
        let mut model = composition();
        model.pick(3);
        model.set_example(5);
        assert_eq!(selected(&model), (1, 3));
        assert_eq!(model.route.start(), 0);
    }

    #[test]
    fn a_link_reproduces_the_selection_and_the_path() {
        let mut model = composition();
        model.select(2, 3);
        model.set_path(Path::Stages);
        model.homotopy = 0.25;
        let fragment = model.scene().to_fragment();
        assert!(fragment.contains("stages=2-3"), "{fragment}");
        assert!(fragment.contains("path=stages"), "{fragment}");
        let reopened = opened(&fragment);
        assert_eq!(reopened.route, model.route);
        assert_eq!(reopened.homotopy, 0.25);
        assert_eq!(reopened.new_function, Some(expr::chain_wgsl(&model.chain)));
    }

    #[test]
    fn a_link_to_stages_that_do_not_exist_selects_them_all() {
        assert_eq!(selected(&opened("f=z^2&stages=1")), (1, 1));
        assert_eq!(selected(&opened("f=sin(z^2)&stages=2")), (2, 2));
        for link in ["f=z^2&stages=2", "f=sin(z^2)&stages=2-1", "f=sin(z^2)&stages=0-1", "f=sin(&stages=2"] {
            let model = opened(link);
            assert!(model.route.is_all(model.count()), "{link}");
        }
    }

    #[test]
    fn a_link_from_before_stages_shows_the_straight_line() {
        let model = opened("ex=joukowski&color=stretch&t=0.5");
        assert!(model.route.is_all(model.count()));
        assert_eq!((model.route.path, model.homotopy), (Path::Straight, 0.5));
    }

    /// What the patch draws at `offset` screen points from the window centre,
    /// and where the map itself puts that grid point, in f64.
    fn patch_and_map(model: &Model, offset: Complex<f64>) -> (Complex<f64>, Complex<f64>) {
        let patch = model.patch().expect("a patch");
        let u = offset / patch.cubic[1];
        let [a0, a1, a2, a3] = patch.cubic;
        let drawn = a0 + u * (a1 + u * (a2 + u * a3));
        let z = patch.source + u * patch.units_per_u;
        let position = Complex::new(model.position.x, model.position.y);
        (drawn, (model.map(z) + position) * model.pixels_per_unit)
    }

    #[test]
    fn far_zoomed_in_the_grid_comes_from_a_patch() {
        let mut model = composition();
        // Aim the window at where the point lands.
        let w = model.map(Complex::new(0.3, 0.7));
        model.position = dvec2(-w.re, -w.im);
        model.pixels_per_unit = 1e3;
        assert_eq!(model.patch(), None);
        model.pixels_per_unit = 1e10;
        for offset in [Complex::new(0.0, 0.0), Complex::new(300.0, -200.0)] {
            let (drawn, exact) = patch_and_map(&model, offset);
            assert!((drawn - exact).norm() < 0.5, "{drawn} vs {exact}");
        }
    }

    #[test]
    fn a_patch_follows_its_point_from_one_view_to_the_next() {
        let mut model = composition();
        let w = model.map(Complex::new(0.3, 0.7));
        model.position = dvec2(-w.re, -w.im);
        model.pixels_per_unit = 1e10;
        model.patch_seed = model.patch().map(|patch| patch.source);
        let before = model.patch_seed.unwrap();
        model.position += dvec2(3e-8, -2e-8);
        let after = model.patch().unwrap().source;
        assert!((after - before).norm() < 1e-6, "{before} to {after}");
    }

    #[test]
    fn the_unmapped_grid_is_patched_at_any_depth() {
        let mut model = opened("f=z&t=1");
        model.position = dvec2(-1.0, -1.0);
        model.pixels_per_unit = 1e13;
        let patch = model.patch().unwrap();
        assert_eq!(patch.source, Complex::new(1.0, 1.0));
        assert_eq!(patch.layout.h_lines + patch.layout.v_lines, 2);
        // Near the origin the numbers stay good far past any f32.
        model.position = dvec2(1e-200, 0.0);
        model.pixels_per_unit = 1e205;
        assert!(model.patch().is_some());
    }

    #[test]
    fn peeking_draws_the_source_grid_from_a_patch_too() {
        let mut model = composition();
        model.position = dvec2(-0.3, -0.7);
        model.pixels_per_unit = 1e10;
        model.peek = true;
        let patch = model.patch().unwrap();
        assert_eq!(patch.source, Complex::new(0.3, 0.7));
        assert!((patch.cubic[1] - 1.0).norm() < 1e-12);
    }

    #[test]
    fn a_critical_point_keeps_the_plain_grid() {
        let mut model = opened("f=z^2&t=1");
        model.position = DVec2::ZERO;
        model.pixels_per_unit = 1e10;
        assert_eq!(model.patch(), None);
    }

    #[test]
    fn the_zoom_reads_short_at_any_size() {
        assert_eq!(zoom_text(150.0), "150.00");
        assert_eq!(zoom_text(1.5e9), "1.500e9");
        assert_eq!(zoom_text(2e-9), "2.000e-9");
    }

    #[test]
    fn peeking_shows_the_source_grid_in_any_stage() {
        let mut model = composition();
        let z = Complex::new(0.3, 0.7);
        model.pick(3);
        model.peek = true;
        assert!(model.unmapped());
        assert_eq!(model.map(z), z);
        assert_eq!(model.map_derivative(z), Complex::new(1.0, 0.0));
    }

    /// Unmapped, the layout may leave out what is off screen; mapped, it may not.
    #[test]
    fn only_a_grid_at_the_identity_is_clipped_to_the_view() {
        let mut model = composition();
        model.pixels_per_unit = 2000.0;
        let clipped = |model: &Model| model.grid_layout() != model.full_layout();

        model.pick(1);
        model.homotopy = 0.0;
        assert!(model.unmapped() && clipped(&model));
        model.pick(2);
        model.homotopy = 0.0;
        assert!(!model.unmapped() && !clipped(&model));
        model.peek = true;
        assert!(clipped(&model));
    }

    #[test]
    fn edges_follow_the_cursor_only_on_an_unmapped_grid() {
        let mut model = composition();
        model.pixels_per_unit = 2000.0;
        model.pick(2);
        model.homotopy = 0.0;
        assert_eq!(model.edge_units_per_point(), 1.0 / DEFAULT_PIXELS_PER_UNIT);
        model.pick(1);
        model.homotopy = 0.0;
        assert_eq!(model.edge_units_per_point(), 1.0 / 2000.0);
    }

    #[test]
    fn borders_are_found_where_a_stage_draws_them() {
        // 2z, stage 1 of sin(2z), finished: the right border is at x = 2.
        let mut model = opened("f=sin(2z)&grid=-1,1,-1,1&stages=1&t=1");
        model.pixels_per_unit = 100.0;
        let (border, z) = model.nearest_border(vec2(205.0, 40.0)).unwrap();
        assert_eq!(border, Border::XMax);
        assert!((z - Complex::new(1.0, 0.2)).norm() < 1e-3, "{z}");
    }

    #[test]
    fn canvas_to_egui_flips_to_top_left() {
        let size = vec2(390.0, 844.0);
        assert_eq!(canvas_to_egui(Vec2::ZERO, size), egui::pos2(195.0, 422.0));
        assert_eq!(canvas_to_egui(vec2(-195.0, 422.0), size), egui::pos2(0.0, 0.0));
        assert_eq!(canvas_to_egui(vec2(195.0, -422.0), size), egui::pos2(390.0, 844.0));
        let pos = vec2(-12.5, 100.0);
        assert_eq!(egui_to_canvas(canvas_to_egui(pos, size), size), pos);
    }
}
