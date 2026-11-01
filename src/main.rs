use bevy_input::mouse::MouseScrollUnit;
use conformal_visualizer::create_gridlines;
use nannou::prelude::*;
use num::Complex;

/// The function being visualized. Change this to look at a different map.
///
/// `p` is the complex parameter controlled with the XY pad in the control window.
/// See <https://docs.rs/num-complex/latest/num_complex/struct.Complex.html>
/// for the available functions (`exp`, `ln`, `powc`, `sin`, ...).
fn f(z: Complex<f32>, p: Complex<f32>) -> Complex<f32> {
    (1.0 - z) / (1.0 + z) * p
}

/// Shown in the control window; keep in sync with [`f`].
const FUNCTION_LABEL: &str = "f(z) = p · (1 − z) / (1 + z)";

/// Number of points per grid segment, so lines bend smoothly under `f`.
const SEGMENT_RESOLUTION: f32 = 4.0;
/// Points per scroll "line" for mice that report wheel clicks instead of pixels.
const POINTS_PER_LINE: f32 = 24.0;
/// Drop polyline points beyond this distance from the origin (poles of `f`).
const MAX_SCREEN_COORD: f32 = 1.0e5;
const MAX_RESOLUTION: f32 = 6.0;
const SCALE_RANGE: std::ops::RangeInclusive<f32> = 1.0..=20_000.0;

fn main() {
    nannou::app(model).update(update).run();
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
    /// Last cursor position while dragging the canvas.
    drag_from: Option<Vec2>,
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
        pixels_per_unit: 150.0,
        position: Vec2::ZERO,
        apply_function: true,
        resolution: 2.0,
        x_min: 0.0,
        x_max: 1.0,
        y_min: 0.0,
        y_max: 1.0,
        parameter: Complex::new(1.0, 0.0),
        homotopy: 1.0,
        drag_from: None,
    }
}

fn update(app: &App, model: &mut Model) {
    let ctx = app.egui_for_window(model.window);

    egui::Window::new("Conformal Map")
        .anchor(egui::Align2::LEFT_TOP, [10.0, 10.0])
        .resizable(false)
        .show(&ctx, |ui| controls(ui, model));

    // Drag the canvas to pan, unless the drag started on the UI.
    let buttons = app.mouse_buttons();
    let mouse = app.mouse();
    if buttons.just_pressed(MouseButton::Left) && !ctx.is_pointer_over_egui() {
        model.drag_from = Some(mouse);
    }
    if !buttons.pressed(MouseButton::Left) {
        model.drag_from = None;
    }
    if let Some(from) = model.drag_from {
        model.position += (mouse - from) / model.pixels_per_unit;
        model.drag_from = Some(mouse);
    }
}

fn controls(ui: &mut egui::Ui, model: &mut Model) {
    ui.monospace(FUNCTION_LABEL);
    ui.separator();

    ui.add(
        egui::Slider::new(&mut model.pixels_per_unit, SCALE_RANGE)
            .logarithmic(true)
            .text("Scale"),
    );
    ui.checkbox(&mut model.apply_function, "Apply function (space)");
    ui.add(egui::Slider::new(&mut model.homotopy, 0.0..=1.0).text("Homotopy"));
    ui.add(egui::Slider::new(&mut model.resolution, 0.0..=MAX_RESOLUTION).text("Resolution"));

    ui.separator();
    ui.label("Grid domain");
    for (value, label) in [
        (&mut model.x_min, "x min"),
        (&mut model.x_max, "x max"),
        (&mut model.y_min, "y min"),
        (&mut model.y_max, "y max"),
    ] {
        ui.add(egui::Slider::new(value, -15.0..=15.0).text(label));
    }

    ui.separator();
    ui.horizontal(|ui| {
        ui.label("Parameter p");
        if ui.button("Reset (r)").clicked() {
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

    ui.separator();
    ui.small(
        "scroll / drag: pan · shift+scroll: zoom · ctrl+scroll: resolution\n\
         hold w/a/s/d + scroll: move that edge of the grid",
    );
}

fn key_pressed(app: &App, model: &mut Model, key: KeyCode) {
    if app
        .egui_for_window(model.window)
        .egui_wants_keyboard_input()
    {
        return;
    }
    match key {
        KeyCode::Space => model.apply_function = !model.apply_function,
        KeyCode::KeyR => model.parameter = Complex::new(1.0, 0.0),
        _ => {}
    }
}

fn mouse_wheel(app: &App, model: &mut Model, wheel: MouseWheel) {
    if app.egui_for_window(model.window).is_pointer_over_egui() {
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
    // Positive y moves content down; the plane has y pointing up.
    let plane = vec2(delta.x, -delta.y) / model.pixels_per_unit;

    let keys = app.keys();
    let any = |a, b| keys.pressed(a) || keys.pressed(b);
    let (w, a, s, d) = (
        keys.pressed(KeyCode::KeyW),
        keys.pressed(KeyCode::KeyA),
        keys.pressed(KeyCode::KeyS),
        keys.pressed(KeyCode::KeyD),
    );

    if any(KeyCode::ShiftLeft, KeyCode::ShiftRight) {
        let zoomed = model.pixels_per_unit * (dominant / 200.0).exp();
        model.pixels_per_unit = zoomed.clamp(*SCALE_RANGE.start(), *SCALE_RANGE.end());
    } else if any(KeyCode::ControlLeft, KeyCode::ControlRight) {
        model.resolution = (model.resolution - dominant / 50.0).clamp(0.0, MAX_RESOLUTION);
    } else if w || a || s || d {
        if a {
            model.x_min += plane.x;
        }
        if d {
            model.x_max += plane.x;
        }
        if w {
            model.y_max += plane.y;
        }
        if s {
            model.y_min += plane.y;
        }
    } else {
        model.position += plane;
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

    let (points, line_structure) = create_gridlines(
        SEGMENT_RESOLUTION,
        model.resolution,
        model.x_min,
        model.x_max,
        model.y_min,
        model.y_max,
    );
    let mut points = points.map(|z| {
        let w = if model.apply_function {
            z + (f(z, model.parameter) - z) * model.homotopy
        } else {
            z
        };
        to_screen(model, w)
    });

    let polyline = |segment: &mut Vec<Vec2>| {
        if segment.len() >= 2 {
            draw.polyline()
                .weight(1.5)
                .color(WHITE)
                .points(std::mem::take(segment));
        } else {
            segment.clear();
        }
    };
    for line_len in line_structure {
        // Split each line where it runs off to infinity (e.g. through a pole).
        let mut segment = Vec::with_capacity(line_len);
        for p in points.by_ref().take(line_len) {
            if p.is_finite() && p.length() < MAX_SCREEN_COORD {
                segment.push(p);
            } else {
                polyline(&mut segment);
            }
        }
        polyline(&mut segment);
    }
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
