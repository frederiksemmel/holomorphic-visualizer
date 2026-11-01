use conformal_visualizer::create_gridlines;
use eframe::egui::{self, Color32, Key, MouseWheelUnit, Pos2, Rect, Sense, Stroke, Vec2, vec2};
use num::Complex;

/// The function being visualized. Change this to look at a different map.
///
/// `p` is the complex parameter controlled with the XY pad in the side panel.
/// See <https://docs.rs/num-complex/latest/num_complex/struct.Complex.html>
/// for the available functions (`exp`, `ln`, `powc`, `sin`, ...).
fn f(z: Complex<f32>, p: Complex<f32>) -> Complex<f32> {
    (1.0 - z) / (1.0 + z) * p
}

/// Shown in the side panel; keep in sync with [`f`].
const FUNCTION_LABEL: &str = "f(z) = p · (1 − z) / (1 + z)";

/// Number of points per grid segment, so lines bend smoothly under `f`.
const SEGMENT_RESOLUTION: f32 = 4.0;
/// Points per scroll "line" for mice that report wheel clicks instead of pixels.
const POINTS_PER_LINE: f32 = 24.0;
/// Drop polyline points beyond this distance from the origin (poles of `f`).
const MAX_SCREEN_COORD: f32 = 1.0e5;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Conformal Map Visualizer")
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    eframe::run_native(
        "conformal-visualizer",
        options,
        Box::new(|_cc| Ok(Box::new(Visualizer::default()))),
    )
}

struct Visualizer {
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
}

impl Default for Visualizer {
    fn default() -> Self {
        Self {
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
        }
    }
}

impl eframe::App for Visualizer {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_keys(ui.ctx());

        egui::Panel::left("controls")
            .resizable(false)
            .show_inside(ui, |ui| self.controls(ui));

        egui::CentralPanel::no_frame()
            .frame(egui::Frame::NONE.fill(Color32::from_gray(5)))
            .show_inside(ui, |ui| self.canvas(ui));
    }
}

impl Visualizer {
    fn handle_keys(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            if i.key_pressed(Key::Space) {
                self.apply_function = !self.apply_function;
            }
            if i.key_pressed(Key::R) {
                self.parameter = Complex::new(1.0, 0.0);
            }
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.heading("Conformal Map");
        ui.monospace(FUNCTION_LABEL);
        ui.separator();

        ui.add(
            egui::Slider::new(&mut self.pixels_per_unit, 1.0..=20_000.0)
                .logarithmic(true)
                .text("Scale"),
        );
        ui.checkbox(&mut self.apply_function, "Apply function (space)");
        ui.add(egui::Slider::new(&mut self.homotopy, 0.0..=1.0).text("Homotopy"));
        ui.add(egui::Slider::new(&mut self.resolution, 0.0..=6.0).text("Resolution"));

        ui.separator();
        ui.label("Grid domain");
        for (value, label) in [
            (&mut self.x_min, "x min"),
            (&mut self.x_max, "x max"),
            (&mut self.y_min, "y min"),
            (&mut self.y_max, "y max"),
        ] {
            ui.add(egui::Slider::new(value, -15.0..=15.0).text(label));
        }

        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Parameter p");
            if ui.button("Reset (r)").clicked() {
                self.parameter = Complex::new(1.0, 0.0);
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::DragValue::new(&mut self.parameter.re)
                    .speed(0.01)
                    .prefix("re "),
            );
            ui.add(
                egui::DragValue::new(&mut self.parameter.im)
                    .speed(0.01)
                    .prefix("im "),
            );
        });
        xy_pad(ui, &mut self.parameter, 3.0);

        ui.separator();
        ui.small(
            "scroll: pan · shift+scroll: zoom · ctrl+scroll: resolution\n\
             hold w/a/s/d + scroll: move that edge of the grid",
        );
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::drag());
        let rect = response.rect;

        if response.dragged() {
            let d = response.drag_delta() / self.pixels_per_unit;
            self.position += vec2(d.x, -d.y);
        }
        if response.hovered() {
            self.handle_scroll(ui.ctx());
        }

        let to_screen = |z: Complex<f32>| self.to_screen(rect, z);

        // Coordinate axes from -1 to 1.
        let axis = Stroke::new(1.0_f32, Color32::from_gray(160));
        let head = (0.1 * self.pixels_per_unit, 0.05 * self.pixels_per_unit);
        arrow(
            &painter,
            to_screen(Complex::new(-1.0, 0.0)),
            to_screen(Complex::new(1.0, 0.0)),
            head,
            axis,
        );
        arrow(
            &painter,
            to_screen(Complex::new(0.0, -1.0)),
            to_screen(Complex::new(0.0, 1.0)),
            head,
            axis,
        );

        let (points, line_structure) = create_gridlines(
            SEGMENT_RESOLUTION,
            self.resolution,
            self.x_min,
            self.x_max,
            self.y_min,
            self.y_max,
        );
        let mut points = points.map(|z| {
            let w = if self.apply_function {
                z + (f(z, self.parameter) - z) * self.homotopy
            } else {
                z
            };
            to_screen(w)
        });

        let stroke = Stroke::new(1.5_f32, Color32::WHITE);
        for line_len in line_structure {
            // Split each line where it runs off to infinity (e.g. through a pole).
            let mut segment = Vec::with_capacity(line_len);
            for p in points.by_ref().take(line_len) {
                if p.is_finite() && p.to_vec2().length() < MAX_SCREEN_COORD {
                    segment.push(p);
                } else if !segment.is_empty() {
                    flush(&painter, &mut segment, stroke);
                }
            }
            flush(&painter, &mut segment, stroke);
        }
    }

    fn handle_scroll(&mut self, ctx: &egui::Context) {
        let (events, keys) = ctx.input(|i| {
            let held = |k| i.key_down(k);
            (
                i.events.clone(),
                [held(Key::W), held(Key::A), held(Key::S), held(Key::D)],
            )
        });
        let [w, a, s, d] = keys;

        for event in events {
            let egui::Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            let delta = match unit {
                MouseWheelUnit::Point => delta,
                MouseWheelUnit::Line => delta * POINTS_PER_LINE,
                MouseWheelUnit::Page => delta * 20.0 * POINTS_PER_LINE,
            };
            // Some platforms turn shift+wheel into horizontal scrolling.
            let dominant = if delta.y.abs() >= delta.x.abs() {
                delta.y
            } else {
                delta.x
            };
            // Convert to plane units, with y pointing up.
            let plane = vec2(delta.x, -delta.y) / self.pixels_per_unit;

            if modifiers.shift {
                self.pixels_per_unit =
                    (self.pixels_per_unit * (dominant / 200.0).exp()).clamp(1.0, 20_000.0);
            } else if modifiers.ctrl || modifiers.command {
                self.resolution = (self.resolution - dominant / 50.0).clamp(0.0, 6.0);
            } else if w || a || s || d {
                if a {
                    self.x_min += plane.x;
                }
                if d {
                    self.x_max += plane.x;
                }
                if w {
                    self.y_max += plane.y;
                }
                if s {
                    self.y_min += plane.y;
                }
            } else {
                self.position += plane;
            }
        }
    }

    fn to_screen(&self, rect: Rect, z: Complex<f32>) -> Pos2 {
        let p = (vec2(z.re, z.im) + self.position) * self.pixels_per_unit;
        rect.center() + vec2(p.x, -p.y)
    }
}

fn flush(painter: &egui::Painter, segment: &mut Vec<Pos2>, stroke: Stroke) {
    if segment.len() >= 2 {
        painter.line(std::mem::take(segment), stroke);
    } else {
        segment.clear();
    }
}

fn arrow(
    painter: &egui::Painter,
    from: Pos2,
    to: Pos2,
    (length, width): (f32, f32),
    stroke: Stroke,
) {
    painter.line_segment([from, to], stroke);
    let dir = (to - from).normalized();
    let normal = dir.rot90();
    let base = to - dir * length;
    painter.line_segment([to, base + normal * width], stroke);
    painter.line_segment([to, base - normal * width], stroke);
}

/// A square pad for dragging a complex value within `[-range, range]²`.
fn xy_pad(ui: &mut egui::Ui, value: &mut Complex<f32>, range: f32) {
    let size = ui.available_width().min(200.0);
    let (response, painter) = ui.allocate_painter(Vec2::splat(size), Sense::click_and_drag());
    let rect = response.rect;
    let visuals = ui.style().interact(&response);

    if let Some(pos) = response.interact_pointer_pos() {
        let t = (pos - rect.min) / rect.size();
        value.re = egui::lerp(-range..=range, t.x.clamp(0.0, 1.0));
        value.im = egui::lerp(range..=-range, t.y.clamp(0.0, 1.0));
    }

    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let faint = Stroke::new(1.0_f32, ui.visuals().weak_text_color());
    painter.line_segment([rect.center_top(), rect.center_bottom()], faint);
    painter.line_segment([rect.left_center(), rect.right_center()], faint);

    let t = vec2(
        egui::remap_clamp(value.re, -range..=range, 0.0..=1.0),
        egui::remap_clamp(value.im, range..=-range, 0.0..=1.0),
    );
    let knob = rect.min + t * rect.size();
    painter.circle_filled(knob, 6.0, visuals.fg_stroke.color);
    painter.text(
        rect.left_top() + vec2(4.0, 4.0),
        egui::Align2::LEFT_TOP,
        format!("{:.2} {:+.2}i", value.re, value.im),
        egui::FontId::monospace(11.0),
        ui.visuals().text_color(),
    );
}
