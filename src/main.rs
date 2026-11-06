use bevy::asset::{Assets, uuid_handle};
use bevy::input::{ButtonInput, mouse::MouseScrollUnit, touch::TouchPhase};
use bevy::platform::time::Instant;
use conformal_visualizer::axes::Step;
use conformal_visualizer::borders::{self, Border};
use conformal_visualizer::examples::EXAMPLES;
use conformal_visualizer::expr::{self, Function, Homotopy, ParseError};
use conformal_visualizer::scene::{Coloring, DEFAULT_PIXELS_PER_UNIT, DEFAULT_RESOLUTION, Scene};
use conformal_visualizer::{COMPLEX_WGSL, GridLayout, clip_to_view, morph, readout};
use nannou::prelude::render::ShaderModel;
use nannou::prelude::*;
use num::Complex;
use std::sync::Arc;
use std::time::Duration;

/// Points per scroll "line" for mice that report wheel clicks instead of pixels.
const POINTS_PER_LINE: f32 = 24.0;
/// Finest grid the controls offer; below the library's hard cap
/// (`conformal_visualizer::MAX_RESOLUTION`) to keep the line count sane.
const MAX_UI_RESOLUTION: f32 = 6.0;
/// The grid edges stay within `[-DOMAIN_LIMIT, DOMAIN_LIMIT]`. Not a limit
/// anyone should meet: it only keeps the arithmetic finite, and f32 has long
/// stopped telling neighbouring grid lines apart out here.
const DOMAIN_LIMIT: f32 = 1e12;
/// Fraction of an edge's size that dragging its field moves it per screen
/// point, so it covers tiny and huge values alike.
const EDGE_DRAG_SPEED: f64 = 0.005;
/// Zoom limits, in screen points per unit: zoomed all the way out, the window
/// spans about `DOMAIN_LIMIT`. Positions are f32, so panning loses about
/// |z| · 1e-7 units; at 1e6 that stays under a pixel for |z| up to ~5.
const SCALE_RANGE: std::ops::RangeInclusive<f32> = 1e-9..=1_000_000.0;
/// Exponent of the zoom per screen point of Ctrl + scroll or of dragging the
/// zoom field. Its range spans too many decades for a slider's track.
const ZOOM_PER_POINT: f32 = 0.005;
/// Scrolling or mouse travel, in screen points, across the whole homotopy range
/// at the default zoom (see [`Model::homotopy_travel`]).
const HOMOTOPY_TRAVEL: f32 = 300.0;
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
const SCROLL_PER_SQUARE: f32 = DEFAULT_PIXELS_PER_UNIT / (1 << DEFAULT_RESOLUTION as u32) as f32;
/// At most this many squares per scroll event, so a flick doesn't throw the
/// border across the plane.
const MAX_SCROLL_SQUARES: f32 = 4.0;
/// The control window fades to this while one of its sliders is dragged, to
/// show the result underneath: on a phone it covers most of the canvas.
const HELD_PANEL_OPACITY: f32 = 0.2;
/// How long that fade takes.
const PANEL_FADE_SECONDS: f32 = 0.1;
/// Shift slows dragging p (on its pad or with the right button) to this.
const FINE: f32 = 0.2;
/// Straight segments per unit length of grid line, so lines bend smoothly under `f`.
const MAX_DENSITY: f32 = 2048.0;
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
const SHADER_SETTLE: f32 = 1.0;
/// Seconds between updates of the page URL. Browsers limit how often a page
/// may rewrite its URL (Safari to 100 times in 30 s), and a slider drag
/// changes the scene every frame.
const LINK_INTERVAL: f32 = 0.4;
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
const READOUT_TOLERANCE: f32 = 0.25;
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
        let ends = Homotopy::of_stage(&function.stages(), 0).expect("stage 0 always exists");
        set_grid_shader(app.world_mut(), &ends.to_wgsl());
    }
}

/// The grid shader with `functions` (generated by `Homotopy::to_wgsl`).
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
    blend: f32,
    peek: u32,
    line_width: f32,
    coloring: u32,
    stretch_octaves: f32,
    domain_min: Vec2,
    domain_max: Vec2,
    half_view: Vec2,
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
    Starting(f32),
    /// Playing; the app time at which the animation was at its start.
    Since(f32),
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
    /// The compositional parts of the last formula that worked, the last of
    /// them being all of it (see `Function::stages`).
    stages: Vec<Function>,
    /// Which of them shows, counted from 1, or 0 for the whole function.
    stage: usize,
    /// What the homotopy runs between: the functions the shader draws, for the CPU.
    ends: Homotopy,
    /// WGSL for new `ends`, to be put in the grid shader on the next update.
    new_function: Option<String>,
    /// App time until which to keep redrawing (see [`SHADER_SETTLE`]).
    settling_until: f32,
    /// Screen points per unit length in the complex plane.
    pixels_per_unit: f32,
    /// Offset of the complex plane, in plane units.
    position: Vec2,
    /// Grid density: `2^resolution` lines per unit.
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    parameter: Complex<f32>,
    /// Blend between the two `ends`: the start (0) and the end (1).
    homotopy: f32,
    morph: Morph,
    coloring: Coloring,
    /// The fragment last put in the page URL, and the app time it was put there.
    link_written: (String, f32),
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
    scrolled_border: Option<(Border, Complex<f32>)>,
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
            let half = self.half_view / self.pixels_per_unit;
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

    fn domain(&self) -> (Complex<f32>, Complex<f32>) {
        (
            Complex::new(self.x_min, self.y_min),
            Complex::new(self.x_max, self.y_max),
        )
    }

    fn layout(resolution: f32, min: Complex<f32>, max: Complex<f32>) -> GridLayout {
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
    fn blend(&self) -> f32 {
        if self.peek { 0.0 } else { self.homotopy }
    }

    /// The grid is drawn as it is in the plane. A blend of 0 alone doesn't
    /// say so: from stage 3 on the homotopy starts from a mapped grid.
    fn unmapped(&self) -> bool {
        self.peek || self.ends.unmapped(self.homotopy)
    }

    /// Where the grid point `z` is drawn in the plane, as `g` in
    /// `src/grid.wgsl` computes it.
    fn map(&self, z: Complex<f32>) -> Complex<f32> {
        if self.peek {
            z
        } else {
            self.ends.map(z, self.parameter, self.homotopy)
        }
    }

    /// The derivative of [`Model::map`].
    fn map_derivative(&self, z: Complex<f32>) -> Complex<f32> {
        if self.peek {
            Complex::new(1.0, 0.0)
        } else {
            self.ends.derivative(z, self.parameter, self.homotopy)
        }
    }

    /// Where the grid point `z` is drawn on screen, in points.
    fn grid_to_screen(&self, z: Complex<f32>) -> Complex<f32> {
        (self.map(z) + complex(self.position)) * self.pixels_per_unit
    }

    fn grid(&self) -> [f32; 4] {
        [self.x_min, self.x_max, self.y_min, self.y_max]
    }

    /// The grid border drawn nearest the screen point `cursor`, and the grid
    /// point on it that lands there.
    fn nearest_border(&self, cursor: Vec2) -> Option<(Border, Complex<f32>)> {
        borders::nearest_border(self.grid(), &|z| self.grid_to_screen(z), complex(cursor))
    }

    /// Moves `border` by `step` grid units. Like `move_edges`, it may pass
    /// the opposite border, which empties the grid until it comes back.
    fn move_border(&mut self, border: Border, step: f32) {
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
    fn grab_for_scroll(&self, cursor: Vec2) -> Option<(Border, Complex<f32>)> {
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
        let step = delta * self.edge_units_per_point();
        self.move_border(Border::XMin, step.x);
        self.move_border(Border::XMax, step.x);
        self.move_border(Border::YMin, step.y);
        self.move_border(Border::YMax, step.y);
    }

    /// Moves the homotopy by `amount` screen points of travel (see
    /// [`Model::homotopy_travel`]), stopping the animation.
    fn scrub(&mut self, amount: f32) {
        self.morph = Morph::Off;
        self.homotopy = (self.homotopy + amount / self.homotopy_travel()).clamp(0.0, 1.0);
    }

    /// Makes the grid finer by `amount` screen points of drag or scroll
    /// (coarser, if negative).
    fn refine(&mut self, amount: f32) {
        self.resolution =
            (self.resolution + amount / RESOLUTION_TRAVEL).clamp(0.0, MAX_UI_RESOLUTION);
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
            stage: self.stage,
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
        self.position = Vec2::from_array(scene.position);
        self.pixels_per_unit = scene
            .pixels_per_unit
            .clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        // A formula that doesn't parse has no stages to ask for.
        if self.formula_error.is_none() {
            self.show_stage(scene.stage);
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

    /// Shows all of `formula`, if it parses. Either way no stage stays
    /// picked: the one on show may be about to stop existing.
    fn set_formula(&mut self, formula: String) {
        match expr::parse(&formula) {
            Ok(function) => {
                self.stages = function.stages();
                self.show_stage(0);
                self.formula_error = None;
            }
            Err(error) => {
                self.formula_error = Some(error);
                if self.stage != 0 {
                    self.show_stage(0);
                }
            }
        }
        self.formula = formula;
    }

    /// Puts stage `stage` in the shader and on the CPU. Returns whether
    /// there is such a stage; if not, nothing changes.
    fn show_stage(&mut self, stage: usize) -> bool {
        let Some(ends) = Homotopy::of_stage(&self.stages, stage) else {
            return false;
        };
        self.new_function = Some(ends.to_wgsl());
        self.ends = ends;
        self.stage = stage;
        true
    }

    /// Goes to stage `stage`, if it exists, and shows it finished, so that
    /// scrubbing back reveals where it came from. If the animation is
    /// playing it goes on playing, from the start of this stage.
    fn pick_stage(&mut self, stage: usize) {
        if stage == self.stage || !self.show_stage(stage) {
            return;
        }
        (self.homotopy, self.morph) = match self.morph {
            Morph::Off => (1.0, Morph::Off),
            _ => (0.0, Morph::Starting(0.0)),
        };
    }

    /// What the readout and the legend call stage `stage`.
    fn stage_name(stage: usize) -> String {
        match stage {
            0 => "f".into(),
            k => format!("g{k}"),
        }
    }

    fn reset_parameter(&mut self) {
        self.parameter = EXAMPLES[self.example].parameter;
    }

    fn reset_view(&mut self) {
        self.pixels_per_unit = DEFAULT_PIXELS_PER_UNIT;
        self.position = Vec2::ZERO;
    }

    /// Zooms by `factor`, keeping the plane point under `anchor` (screen points) in place.
    fn zoom(&mut self, factor: f32, anchor: Vec2) {
        let under_anchor = anchor / self.pixels_per_unit - self.position;
        self.pixels_per_unit =
            (self.pixels_per_unit * factor).clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
        self.position = anchor / self.pixels_per_unit - under_anchor;
    }

    /// Grid units per screen point for gestures that drag the grid edges.
    /// Unmapped, an edge follows the cursor. Mapped, the zoom says nothing
    /// about how far a source edge moves on screen (and far zoomed in, 1:1
    /// would barely move it), so edges move at the default zoom's rate.
    fn edge_units_per_point(&self) -> f32 {
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
    fn homotopy_travel(&self) -> f32 {
        HOMOTOPY_TRAVEL * (self.pixels_per_unit / DEFAULT_PIXELS_PER_UNIT).max(1.0)
    }

    /// Moves p by `delta` screen points. p is not a point on screen, so this
    /// ignores the zoom.
    fn move_parameter(&mut self, delta: Vec2) {
        let delta = delta / DEFAULT_PIXELS_PER_UNIT;
        self.parameter += Complex::new(delta.x, delta.y);
    }

    /// Moves each grid edge whose key is held (W/A/S/D: top, left, bottom,
    /// right) by `delta` grid units. Returns whether any edge key was held.
    fn move_edges(&mut self, keys: &ButtonInput<KeyCode>, delta: Vec2) -> bool {
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
    let stages = expr::parse(example.formula).expect("examples parse").stages();
    let ends = Homotopy::of_stage(&stages, 0).expect("stage 0 always exists");
    let mut model = Model {
        window,
        example: 0,
        formula: example.formula.into(),
        formula_error: None,
        // GridShaderPlugin starts the shader with this example.
        stages,
        stage: 0,
        ends,
        new_function: None,
        settling_until: 0.0,
        pixels_per_unit: DEFAULT_PIXELS_PER_UNIT,
        position: Vec2::ZERO,
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
        link_written: (fragment.clone(), f32::NEG_INFINITY),
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
    };
    model.set_scene(Scene::from_fragment(&fragment));
    model
}

/// Plays the animation, keeps the page URL showing the scene, and redraws
/// every frame only while either needs it or a key is `steering` something.
/// Otherwise the app waits for input, or until `wake_at` if that comes first,
/// when the hover readout is due.
fn animate(app: &App, model: &mut Model, steering: bool, wake_at: Option<Instant>) {
    let now = app.time();
    match model.morph {
        Morph::Off => {}
        Morph::Starting(homotopy) => {
            model.morph = Morph::Since(now - morph::time_at(homotopy));
            model.homotopy = homotopy;
        }
        Morph::Since(start) => model.homotopy = morph::blend(now - start),
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
        model.settling_until = app.time() + SHADER_SETTLE;
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
        model.scrub(steer * dt * HOMOTOPY_TRAVEL / ARROW_SECONDS);
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
        Drag::Pan => model.position += delta / model.pixels_per_unit,
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
    let mut used = model.move_edges(keys, edge_delta * model.edge_units_per_point());
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

/// The stages of the function, to step through its composition. Each button
/// shows its formula. Phones have no number keys, and the keys stop at 9.
fn stage_picker(ui: &mut egui::Ui, model: &mut Model) {
    ui.label("Stages");
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        if ui.selectable_label(model.stage == 0, "0: full").clicked() {
            picked = Some(0);
        }
        for (i, function) in model.stages.iter().enumerate() {
            let stage = i + 1;
            let label = format!("{stage}: {function}");
            if ui.selectable_label(model.stage == stage, label).clicked() {
                picked = Some(stage);
            }
        }
    });
    if let Some(stage) = picked {
        model.pick_stage(stage);
    }
    hint(ui, "keys: 0 for the full function, 1 – 9 for a stage");
    let about = match model.stage {
        0 => "the homotopy goes from the identity to f".to_owned(),
        1 => "the identity: nothing to blend".to_owned(),
        k => format!(
            "the homotopy goes from {} = {} to {} = {}",
            Model::stage_name(k - 1),
            model.ends.start,
            Model::stage_name(k),
            model.ends.end
        ),
    };
    hint(ui, &about);
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
        let slider = egui::Slider::new(&mut model.homotopy, 0.0..=1.0).text("Homotopy");
        let slider = ui.add(slider);
        model.slider_held |= slider.dragged();
        if slider.changed() {
            model.morph = Morph::Off;
        }
        let mut playing = model.morph != Morph::Off;
        if ui.checkbox(&mut playing, "Animate").changed() {
            model.toggle_morph();
        }
    });
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
                    .prefix("Zoom "),
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
    legend(ui, model.coloring, &Model::stage_name(model.stage));

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
        _ => {
            if let Some(stage) = STAGE_KEYS.iter().position(|&k| k == key) {
                model.pick_stage(stage);
            }
        }
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
                [_] => model.position += (pos - previous) / model.pixels_per_unit,
                [a, b] => {
                    let other = if i == 0 { b.1 } else { a.1 };
                    // The midpoint between the fingers moves by half of this finger's motion.
                    model.position += (pos - previous) / 2.0 / model.pixels_per_unit;
                    let before = previous.distance(other);
                    if before > 1.0 {
                        model.zoom(pos.distance(other) / before, (pos + other) / 2.0);
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
        model.zoom((dominant * ZOOM_PER_POINT).exp(), app.mouse());
    } else if modifiers.shift {
        model.scroll_held_border(app.mouse(), content, wheel.unit == MouseScrollUnit::Line);
    } else if modifiers.alt {
        model.move_grid(content);
    } else {
        model.position += content / model.pixels_per_unit;
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

fn complex(v: Vec2) -> Complex<f32> {
    Complex::new(v.x, v.y)
}

fn to_screen(model: &Model, z: Complex<f32>) -> Vec2 {
    (vec2(z.re, z.im) + model.position) * model.pixels_per_unit
}

fn view(app: &App, model: &Model) {
    let draw = app.draw();
    draw.background().srgb(0.02, 0.02, 0.02);
    plane_grid(&draw, model);

    let layout = model.grid_layout();
    let domain = model.full_layout();
    let segments = layout.segment_count();
    if segments == 0 {
        return;
    }
    let params = GridParams {
        parameter: vec2(model.parameter.re, model.parameter.im),
        offset: model.position,
        grid_min: vec2(layout.min.re, layout.min.im),
        grid_max: vec2(layout.max.re, layout.max.im),
        spacing: layout.spacing,
        h_lines: layout.h_lines,
        h_segments: layout.h_segments,
        v_segments: layout.v_segments,
        pixels_per_unit: model.pixels_per_unit,
        blend: model.blend(),
        peek: model.peek as u32,
        line_width: LINE_WIDTH,
        coloring: model.coloring as u32,
        stretch_octaves: STRETCH_OCTAVES,
        domain_min: vec2(domain.min.re, domain.min.im),
        domain_max: vec2(domain.max.re, domain.max.im),
        half_view: model.half_view,
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
    let ppu = model.pixels_per_unit as f64;
    let major = Step::near(MAJOR_SPACING, ppu);
    let divisions = major.minor_divisions();
    let minor = major.value() / divisions as f64;
    let half = model.half_view;
    // In f64: far zoomed in, f32 rounding would shift lines by up to a pixel.
    let lines = |offset: f32, half: f32| {
        let (offset, reach) = (offset as f64, half as f64 / ppu);
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
fn plane_to_egui(model: &Model, z: Complex<f32>) -> egui::Pos2 {
    canvas_to_egui(to_screen(model, z), 2.0 * model.half_view)
}

/// Numbers on the axes of the background grid, at its major lines: real
/// values on the real axis, multiples of i on the imaginary one, 0 once at
/// the origin. An axis off screen keeps its numbers at that edge of the
/// window. Numbers that would land on the control window or on each other
/// are left out.
fn axis_labels(ctx: &egui::Context, model: &Model) {
    let painter = ctx.layer_painter(egui::LayerId::background());
    let ppu = model.pixels_per_unit as f64;
    let step = Step::near(MAJOR_SPACING, ppu);
    let size = 2.0 * model.half_view;
    let (position, half) = (model.position, model.half_view);
    // In f64, as in `plane_grid`.
    let x_at = |x: f64| ((x + position.x as f64) * ppu) as f32 + half.x;
    let y_at = |y: f64| half.y - ((y + position.y as f64) * ppu) as f32;
    let visible = |offset: f32, half: f32| {
        let (offset, reach) = (offset as f64, half as f64 / ppu);
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
    let w = complex(cursor / ppu - model.position);
    let t = model.blend();
    let unmapped = model.unmapped();
    let (min, max) = model.domain();
    let (sources, count) = if unmapped {
        let inside = (min.re..=max.re).contains(&w.re) && (min.im..=max.im).contains(&w.im);
        (if inside { vec![w] } else { Vec::new() }, inside as usize)
    } else {
        // Not finer than f32 can tell apart near w.
        let tolerance = (READOUT_TOLERANCE / ppu).max(8.0 * f32::EPSILON * w.norm().max(1.0));
        let map = |z| model.map(z);
        let derivative = |z| model.map_derivative(z);
        readout::preimages(map, derivative, (min, max), w, tolerance, READOUT_SOURCES)
    };

    let visuals = &ctx.global_style().visuals;
    let (strong, weak) = (visuals.text_color(), visuals.weak_text_color());
    let mut text = MathText::new(READOUT_SIZE);
    let magnitude = |z: Complex<f32>| z.re.abs().max(z.im.abs());
    text.math(
        &if unmapped {
            "z = ".to_owned()
        } else if t == 1.0 {
            format!("{}(z) = ", Model::stage_name(model.stage))
        } else if t == 0.0 {
            format!("{}(z) = ", Model::stage_name(model.stage.saturating_sub(1)))
        } else {
            "w = ".to_owned()
        },
        strong,
    );
    let decimals = readout::decimals(ppu, magnitude(w));
    text.math(&readout::complex(w, decimals), strong);
    if !unmapped && t != 0.0 && t != 1.0 {
        text.words("   at ", weak);
        text.math(&format!("t = {t:.2}"), weak);
    }
    for &z in &sources {
        if !unmapped {
            // As finely as one screen point at w tells apart near z.
            let points_per_unit = ppu * model.map_derivative(z).norm();
            let decimals = readout::decimals(points_per_unit, magnitude(z));
            text.math(&format!("\nz = {}", readout::complex(z, decimals)), strong);
        }
        let df = model.ends.end.derivative(z, model.parameter);
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

/// A field for a grid edge. The edges have no useful bounds, so a slider has
/// no track to offer: this drags at a speed that follows the value, which
/// reaches any size quickly and stays fine near zero, and takes typed values.
fn edge_field(ui: &mut egui::Ui, edge: &mut f32) -> egui::Response {
    let speed = EDGE_DRAG_SPEED * (edge.abs() as f64).max(1.0);
    ui.add(
        egui::DragValue::new(edge)
            .speed(speed)
            .range(-DOMAIN_LIMIT..=DOMAIN_LIMIT)
            .max_decimals(4),
    )
}

/// A square pad for dragging a complex value within `[-range, range]²`.
fn xy_pad(ui: &mut egui::Ui, value: &mut Complex<f32>, range: f32) -> egui::Response {
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
        value.re = egui::lerp(-range..=range, t.x.clamp(0.0, 1.0));
        value.im = egui::lerp(range..=-range, t.y.clamp(0.0, 1.0));
    } else if response.dragged() {
        let speed = if shift { FINE } else { 1.0 };
        let delta = response.drag_delta() * 2.0 * range / rect.width() * speed;
        *value += Complex::new(delta.x, -delta.y);
    }

    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let faint = egui::Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    painter.line_segment([rect.center_top(), rect.center_bottom()], faint);
    painter.line_segment([rect.left_center(), rect.right_center()], faint);

    let t = egui::vec2(
        egui::remap_clamp(value.re, -range..=range, 0.0..=1.0),
        egui::remap_clamp(value.im, range..=-range, 0.0..=1.0),
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
    fn validate_grid_shader(ends: &Homotopy) {
        let source = grid_shader_source(&ends.to_wgsl())
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
    fn grid_shader_compiles_for_every_stage_of_every_example() {
        let formulas = EXAMPLES.iter().map(|example| example.formula);
        for formula in formulas.chain(["sin(cos(p*z))", "exp(z + 1/z)", "p", "z"]) {
            let stages = expr::parse(formula).unwrap().stages();
            for stage in 0..=stages.len() {
                validate_grid_shader(&Homotopy::of_stage(&stages, stage).unwrap());
            }
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

    #[test]
    fn digits_pick_stages_and_0_goes_back() {
        let mut model = composition();
        assert_eq!(model.stages.len(), 4);
        assert_eq!(model.stage, 0);
        model.pick_stage(3);
        assert_eq!(model.stage, 3);
        assert_eq!(model.ends.start.to_string(), "p·z");
        assert_eq!(model.ends.end.to_string(), "cos(p·z)");
        assert_eq!(model.new_function, Some(model.ends.to_wgsl()));
        model.pick_stage(0);
        assert_eq!(model.stage, 0);
        assert!(model.ends.start.is_identity());
        assert_eq!(model.ends.end.to_string(), "sin(cos(p·z))");
    }

    #[test]
    fn a_stage_that_does_not_exist_is_ignored() {
        let mut model = composition();
        model.pick_stage(2);
        model.new_function = None;
        model.homotopy = 0.5;
        model.pick_stage(9);
        assert_eq!((model.stage, model.homotopy), (2, 0.5));
        assert_eq!(model.new_function, None);
    }

    #[test]
    fn a_picked_stage_shows_finished_unless_the_animation_plays() {
        let mut model = composition();
        model.homotopy = 0.25;
        model.pick_stage(2);
        assert_eq!((model.homotopy, model.morph), (1.0, Morph::Off));

        model.morph = Morph::Since(3.0);
        model.pick_stage(3);
        assert_eq!((model.homotopy, model.morph), (0.0, Morph::Starting(0.0)));
    }

    #[test]
    fn more_than_nine_stages_can_all_be_picked() {
        let mut model = opened("");
        model.set_formula(format!("{}z{}", "sin(".repeat(11), ")".repeat(11)));
        assert_eq!(model.stages.len(), 12);
        model.pick_stage(12);
        assert_eq!(model.stage, 12);
    }

    #[test]
    fn editing_the_formula_goes_back_to_the_whole_function() {
        let mut model = composition();
        model.pick_stage(4);
        model.set_formula("z^2".into());
        assert_eq!(model.stage, 0);
        assert_eq!(model.stages.len(), 2);
        assert_eq!(model.ends.end.to_string(), "z^2");
    }

    #[test]
    fn a_formula_with_an_error_shows_the_last_one_whole() {
        let mut model = composition();
        model.pick_stage(3);
        model.set_formula("sin(".into());
        assert!(model.formula_error.is_some());
        assert_eq!(model.stage, 0);
        assert_eq!(model.ends.end.to_string(), "sin(cos(p·z))");
        assert_eq!(model.new_function, Some(model.ends.to_wgsl()));
        // Nothing to rebuild while it stays broken.
        model.new_function = None;
        model.set_formula("sin(z".into());
        assert_eq!(model.new_function, None);
    }

    #[test]
    fn picking_an_example_goes_back_to_the_whole_function() {
        let mut model = composition();
        model.pick_stage(3);
        model.set_example(5);
        assert_eq!(model.stage, 0);
        assert_eq!(model.stages.len(), 4);
        assert!(model.ends.start.is_identity());
    }

    #[test]
    fn a_link_reproduces_the_stage() {
        let mut model = composition();
        model.pick_stage(3);
        model.homotopy = 0.25;
        let fragment = model.scene().to_fragment();
        assert!(fragment.contains("stage=3"), "{fragment}");
        let reopened = opened(&fragment);
        assert_eq!(reopened.stage, 3);
        assert_eq!(reopened.ends, model.ends);
        assert_eq!(reopened.homotopy, 0.25);
        // The stage reaches the shader, not the whole formula queued before it.
        assert_eq!(reopened.new_function, Some(model.ends.to_wgsl()));
    }

    #[test]
    fn a_link_to_a_stage_that_does_not_exist_shows_the_whole_function() {
        assert_eq!(opened("f=z^2&stage=2").stage, 2);
        assert_eq!(opened("f=z^2&stage=3").stage, 0);
        assert_eq!(opened("f=sin(&stage=2").stage, 0);
    }

    #[test]
    fn a_link_from_before_stages_shows_the_whole_function() {
        let model = opened("ex=joukowski&color=stretch&t=0.5");
        assert_eq!((model.stage, model.homotopy), (0, 0.5));
        assert!(model.ends.start.is_identity());
    }

    #[test]
    fn a_stage_starts_where_the_one_before_ended() {
        let mut model = composition();
        let z = Complex::new(0.3, 0.7);
        model.pick_stage(3);
        model.homotopy = 0.0;
        let start = model.map(z);
        model.pick_stage(2);
        assert_eq!(model.homotopy, 1.0);
        let end = model.map(z);
        assert!((start - end).norm() < 1e-6, "{start} vs {end}");
    }

    #[test]
    fn peeking_shows_the_source_grid_in_any_stage() {
        let mut model = composition();
        let z = Complex::new(0.3, 0.7);
        model.pick_stage(3);
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

        model.pick_stage(2);
        model.homotopy = 0.0;
        assert!(model.unmapped() && clipped(&model));
        model.pick_stage(1);
        assert!(model.unmapped() && clipped(&model));
        model.pick_stage(3);
        model.homotopy = 0.0;
        assert!(!model.unmapped() && !clipped(&model));
        model.peek = true;
        assert!(clipped(&model));
    }

    #[test]
    fn edges_follow_the_cursor_only_on_an_unmapped_grid() {
        let mut model = composition();
        model.pixels_per_unit = 2000.0;
        model.pick_stage(3);
        model.homotopy = 0.0;
        assert_eq!(model.edge_units_per_point(), 1.0 / DEFAULT_PIXELS_PER_UNIT);
        model.pick_stage(2);
        model.homotopy = 0.0;
        assert_eq!(model.edge_units_per_point(), 1.0 / 2000.0);
    }

    #[test]
    fn borders_are_found_where_a_stage_draws_them() {
        // 2z, stage 2 of sin(2z), finished: the right border is at x = 2.
        let mut model = opened("f=sin(2z)&grid=-1,1,-1,1&stage=2&t=1");
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
