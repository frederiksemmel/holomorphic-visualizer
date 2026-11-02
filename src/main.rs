use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::input::{ButtonInput, mouse::MouseScrollUnit};
use conformal_visualizer::GridLayout;
use nannou::prelude::render::ShaderModel;
use nannou::prelude::*;
use num::Complex;

/// Shown in the control window; keep in sync with `f` in `src/grid.wgsl`,
/// which is where the function being visualized lives.
const FUNCTION_LABEL: &str = "f(z) = p · (1 − z) / (1 + z)";

/// Points per scroll "line" for mice that report wheel clicks instead of pixels.
const POINTS_PER_LINE: f32 = 24.0;
const MAX_RESOLUTION: f32 = 6.0;
const SCALE_RANGE: std::ops::RangeInclusive<f32> = 1.0..=20_000.0;
const DEFAULT_PIXELS_PER_UNIT: f32 = 150.0;
/// Scrolling or mouse travel, in screen points, across the whole homotopy range.
const HOMOTOPY_TRAVEL: f32 = 300.0;
/// Straight segments per unit length of grid line, so lines bend smoothly under `f`.
const MAX_DENSITY: f32 = 2048.0;
/// Upper bound on segments drawn per frame; large grids get coarser sampling.
const MAX_SEGMENTS: u32 = 1_000_000;
/// In screen points.
const LINE_WIDTH: f32 = 1.5;
/// Matches `STRETCH_OCTAVES` in `src/grid.wgsl`.
const STRETCH_OCTAVES: f32 = 2.0;

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
        load_internal_asset!(app, GRID_SHADER, "grid.wgsl", Shader::from_wgsl);
    }
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
    line_width: f32,
    coloring: u32,
}

/// How grid lines are colored. The discriminants are the `COLORING_*`
/// constants in `src/grid.wgsl`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Coloring {
    #[default]
    White,
    /// Which line of the source grid this is.
    Source,
    /// arg z of the source point, kept through the map.
    Phase,
    /// log |f'(z)|: how much `f` scales lengths here.
    Stretch,
    /// arg f'(z): how far `f` turns directions here.
    Rotation,
}

impl Coloring {
    const ALL: [Coloring; 5] = [
        Coloring::White,
        Coloring::Source,
        Coloring::Phase,
        Coloring::Stretch,
        Coloring::Rotation,
    ];

    fn label(self) -> &'static str {
        match self {
            Coloring::White => "White",
            Coloring::Source => "Source",
            Coloring::Phase => "Phase",
            Coloring::Stretch => "Stretch",
            Coloring::Rotation => "Rotation",
        }
    }

    fn next(self) -> Self {
        Self::ALL[(self as usize + 1) % Self::ALL.len()]
    }
}

struct Model {
    window: Entity,
    /// Screen points per unit length in the complex plane.
    pixels_per_unit: f32,
    /// Offset of the complex plane, in plane units.
    position: Vec2,
    apply_function: bool,
    /// Grid density: `2^resolution` lines per unit.
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    parameter: Complex<f32>,
    /// Blend between the identity (0) and `f` (1).
    homotopy: f32,
    coloring: Coloring,
    /// Mouse button held on the canvas, and the cursor position at the last update.
    drag: Option<(MouseButton, Vec2)>,
    /// The "hold" button in the control window is pressed.
    peek_button: bool,
    /// Show the source grid for now (hold button or X).
    peek: bool,
    /// Cursor position at the last update, for key + mouse-move gestures.
    last_mouse: Vec2,
    fps: f64,
}

impl Model {
    fn grid_layout(&self) -> GridLayout {
        GridLayout::new(
            self.resolution,
            self.x_min,
            self.x_max,
            self.y_min,
            self.y_max,
            MAX_DENSITY,
            MAX_SEGMENTS,
        )
    }

    /// How far to blend from the identity towards `f`; 0 shows the source grid.
    fn blend(&self) -> f32 {
        if self.apply_function && !self.peek {
            self.homotopy
        } else {
            0.0
        }
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

    /// Moves each grid edge whose key is held (W/A/S/D: top, left, bottom,
    /// right) by `delta` plane units. Returns whether any edge key was held.
    fn move_edges(&mut self, keys: &ButtonInput<KeyCode>, delta: Vec2) -> bool {
        let mut held = false;
        for (key, edge, step) in [
            (KeyCode::KeyA, &mut self.x_min, delta.x),
            (KeyCode::KeyD, &mut self.x_max, delta.x),
            (KeyCode::KeyW, &mut self.y_max, delta.y),
            (KeyCode::KeyS, &mut self.y_min, delta.y),
        ] {
            if keys.pressed(key) {
                *edge += step;
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
        .title("Conformal Map Visualizer")
        .size(1200, 800)
        .view(view)
        .key_pressed(key_pressed)
        .mouse_wheel(mouse_wheel)
        .build();
    // Only redraw on input; the picture is static otherwise.
    app.set_update_mode(UpdateMode::wait());

    Model {
        window,
        pixels_per_unit: DEFAULT_PIXELS_PER_UNIT,
        position: Vec2::ZERO,
        apply_function: true,
        resolution: 2.0,
        x_min: 0.0,
        x_max: 1.0,
        y_min: 0.0,
        y_max: 1.0,
        parameter: Complex::new(1.0, 0.0),
        homotopy: 1.0,
        coloring: Coloring::default(),
        drag: None,
        peek_button: false,
        peek: false,
        last_mouse: Vec2::ZERO,
        fps: 0.0,
    }
}

fn update(app: &App, model: &mut Model) {
    let ctx = app.egui_for_window(model.window);
    model.fps = app.fps();

    egui::Window::new("Conformal Map")
        .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
        .resizable(false)
        .show(&ctx, |ui| controls(ui, model));

    let keys = app.keys();
    let typing = ctx.egui_wants_keyboard_input();
    let over_ui = ctx.is_pointer_over_egui();
    model.peek = model.peek_button || (!typing && keys.pressed(KeyCode::KeyX));

    let mouse = app.mouse();
    let moved = mouse - model.last_mouse;
    model.last_mouse = mouse;
    if !typing && !over_ui && model.drag.is_none() && moved != Vec2::ZERO {
        key_gesture(model, &keys, moved);
    }
    mouse_drag(app, model, over_ui);
}

/// Left-drag pans, right-drag moves the parameter p. Drags must start on the canvas.
fn mouse_drag(app: &App, model: &mut Model, over_ui: bool) {
    let buttons = app.mouse_buttons();
    let mouse = app.mouse();
    for button in [MouseButton::Left, MouseButton::Right] {
        if buttons.just_pressed(button) && !over_ui {
            model.drag = Some((button, mouse));
        }
    }
    let Some((button, from)) = model.drag else {
        return;
    };
    if !buttons.pressed(button) {
        model.drag = None;
        return;
    }
    let delta = (mouse - from) / model.pixels_per_unit;
    match button {
        MouseButton::Left => model.position += delta,
        _ => model.parameter += Complex::new(delta.x, delta.y),
    }
    model.drag = Some((button, mouse));
}

/// Hold a key and scroll or move the mouse: W/A/S/D move grid edges (two
/// opposite ones slide the grid), P moves the parameter, F blends the homotopy.
/// `delta` is in screen points with y up. Returns whether a key was held.
fn key_gesture(model: &mut Model, keys: &ButtonInput<KeyCode>, delta: Vec2) -> bool {
    let plane = delta / model.pixels_per_unit;
    let mut used = model.move_edges(keys, plane);
    if keys.pressed(KeyCode::KeyP) {
        model.parameter += Complex::new(plane.x, plane.y);
        used = true;
    }
    if keys.pressed(KeyCode::KeyF) {
        model.homotopy = (model.homotopy + (delta.x + delta.y) / HOMOTOPY_TRAVEL).clamp(0.0, 1.0);
        used = true;
    }
    used
}

/// A faint line under a control that lists its shortcuts.
fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).small().weak());
}

fn controls(ui: &mut egui::Ui, model: &mut Model) {
    ui.monospace(FUNCTION_LABEL);
    ui.separator();

    ui.horizontal(|ui| {
        ui.checkbox(&mut model.apply_function, "Apply function");
        model.peek_button = ui
            .button("Hold to show source")
            .is_pointer_button_down_on();
    });
    hint(ui, "toggle: space · peek at the source grid: hold the button or X");
    ui.add(egui::Slider::new(&mut model.homotopy, 0.0..=1.0).text("Homotopy"));
    hint(ui, "hold F + scroll or move the mouse");

    ui.separator();
    ui.horizontal(|ui| {
        ui.add(
            egui::Slider::new(&mut model.pixels_per_unit, SCALE_RANGE)
                .logarithmic(true)
                .text("Zoom"),
        );
        if ui.button("Reset view").clicked() {
            model.reset_view();
        }
    });
    hint(ui, "zoom: shift + scroll (around the cursor)");
    hint(ui, "pan: drag or scroll · reset view: H");

    ui.separator();
    ui.add(egui::Slider::new(&mut model.resolution, 0.0..=MAX_RESOLUTION).text("Resolution"));
    hint(ui, "finer / coarser: ctrl + scroll");
    ui.label("Grid domain");
    for (value, label) in [
        (&mut model.x_min, "x min"),
        (&mut model.x_max, "x max"),
        (&mut model.y_min, "y min"),
        (&mut model.y_max, "y max"),
    ] {
        ui.add(egui::Slider::new(value, -15.0..=15.0).text(label));
    }
    hint(ui, "hold W / A / S / D (top / left / bottom / right edge)");
    hint(ui, "+ scroll or move the mouse to drag that edge;");
    hint(ui, "A + D or W + S slide the grid, all four move it");

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Color");
        for coloring in Coloring::ALL {
            ui.selectable_value(&mut model.coloring, coloring, coloring.label());
        }
    });
    hint(ui, "C cycles · 1 – 5 picks");
    legend(ui, model.coloring);

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Parameter p");
        if ui.button("Reset").clicked() {
            model.parameter = Complex::new(1.0, 0.0);
        }
    });
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut model.parameter.re)
                .speed(0.01)
                .prefix("re "),
        );
        ui.add(
            egui::DragValue::new(&mut model.parameter.im)
                .speed(0.01)
                .prefix("im "),
        );
    });
    xy_pad(ui, &mut model.parameter, 3.0);
    hint(ui, "drag the pad or right-drag the canvas · reset: R");
    hint(ui, "hold P + scroll or move the mouse");

    ui.separator();
    hint(
        ui,
        &format!(
            "{} segments mapped on the GPU · {:.0} fps",
            model.grid_layout().segment_count(),
            model.fps
        ),
    );
}

/// Explains the current color scheme, using the same palettes as `src/grid.wgsl`.
fn legend(ui: &mut egui::Ui, coloring: Coloring) {
    match coloring {
        Coloring::White => {}
        Coloring::Source => {
            gradient(ui, |t| source(true, t), ["horizontal: bottom", "", "top"]);
            gradient(ui, |t| source(false, t), ["vertical: left", "", "right"]);
            ui.small("Follow a line from the grid to its image.");
        }
        Coloring::Phase => {
            gradient(ui, |t| cyclic(t - 0.5), ["−180°", "0°", "+180°"]);
            ui.small("arg z of the source point: each line keeps the color it had before the map.");
        }
        Coloring::Stretch => {
            let octaves = 2.0f32.powf(STRETCH_OCTAVES);
            gradient(
                ui,
                |t| diverging(2.0 * t - 1.0),
                [&format!("÷{octaves}"), "×1", &format!("×{octaves}")],
            );
            ui.small("|f'(z)|: how much f scales lengths near z.");
        }
        Coloring::Rotation => {
            gradient(ui, |t| cyclic(t - 0.5), ["−180°", "0°", "+180°"]);
            ui.small("arg f'(z): how far f turns directions near z.");
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
        .egui_wants_keyboard_input()
    {
        return;
    }
    const COLORING_KEYS: [KeyCode; 5] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
    ];
    match key {
        KeyCode::Space => model.apply_function = !model.apply_function,
        KeyCode::KeyR => model.parameter = Complex::new(1.0, 0.0),
        KeyCode::KeyH | KeyCode::Home => model.reset_view(),
        KeyCode::KeyC => model.coloring = model.coloring.next(),
        _ => {
            if let Some(i) = COLORING_KEYS.iter().position(|&k| k == key) {
                model.coloring = Coloring::ALL[i];
            }
        }
    }
}

fn mouse_wheel(app: &App, model: &mut Model, wheel: MouseWheel) {
    let ctx = app.egui_for_window(model.window);
    if ctx.is_pointer_over_egui() || ctx.egui_wants_keyboard_input() {
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
    let any = |a, b| keys.pressed(a) || keys.pressed(b);

    if any(KeyCode::ShiftLeft, KeyCode::ShiftRight) {
        model.zoom((dominant / 200.0).exp(), app.mouse());
    } else if any(KeyCode::ControlLeft, KeyCode::ControlRight) {
        model.resolution = (model.resolution - dominant / 50.0).clamp(0.0, MAX_RESOLUTION);
    } else {
        // Positive y moves content down; the plane has y pointing up.
        let content = vec2(delta.x, -delta.y);
        if !key_gesture(model, &keys, content) {
            model.position += content / model.pixels_per_unit;
        }
    }
}

fn to_screen(model: &Model, z: Complex<f32>) -> Vec2 {
    (vec2(z.re, z.im) + model.position) * model.pixels_per_unit
}

fn view(app: &App, model: &Model) {
    let draw = app.draw();
    draw.background().srgb(0.02, 0.02, 0.02);

    // Coordinate axes from -1 to 1.
    let ppu = model.pixels_per_unit;
    for (start, end) in [((-1.0, 0.0), (1.0, 0.0)), ((0.0, -1.0), (0.0, 1.0))] {
        draw.arrow()
            .weight(1.0)
            .head_length(0.1 * ppu)
            .head_width(0.05 * ppu)
            .color(GRAY)
            .start(to_screen(model, Complex::new(start.0, start.1)))
            .end(to_screen(model, Complex::new(end.0, end.1)));
    }

    let layout = model.grid_layout();
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
        line_width: LINE_WIDTH,
        coloring: model.coloring as u32,
    };
    // One quad, drawn once per segment; the shader places its corners.
    let grid = draw.shader_model(GridShader { params });
    let quad = grid.mesh().indexed([Vec3::ZERO; 4], [0, 1, 2, 2, 1, 3]);
    grid.instanced().primitive(quad).range(0..segments);
}

/// A square pad for dragging a complex value within `[-range, range]²`.
fn xy_pad(ui: &mut egui::Ui, value: &mut Complex<f32>, range: f32) {
    let size = ui.available_width().min(200.0);
    let (response, painter) =
        ui.allocate_painter(egui::Vec2::splat(size), egui::Sense::click_and_drag());
    let rect = response.rect;
    let visuals = ui.style().interact(&response);

    if let Some(pos) = response.interact_pointer_pos() {
        let t = (pos - rect.min) / rect.size();
        value.re = egui::lerp(-range..=range, t.x.clamp(0.0, 1.0));
        value.im = egui::lerp(range..=-range, t.y.clamp(0.0, 1.0));
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
}
