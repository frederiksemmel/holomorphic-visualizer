use conformal_visualizer::create_gridlines;
use nannou::prelude::*;
use nannou::ui::prelude::*;
use num::Complex;

fn main() {
    nannou::app(model).update(update).view(view).run();
}

struct Model {
    ui: Ui,
    ids: Ids,
    scale: f32,
    position: Point2,
    apply_function: bool,
    resolution: f32,
    x_min: f32,
    x_max: f32,
    y_min: f32,
    y_max: f32,
    parameter_1: Complex<f32>,
    parameter_homotopy: f32,
}

widget_ids! {
    struct Ids {
        scale,
        apply_function,
        resolution,
        x_min,
        x_max,
        y_min,
        y_max,
        parameter_1,
        parameter_homotopy,
    }
}

fn model(app: &App) -> Model {
    // Set the loop mode to wait for events, an energy-efficient option for pure-GUI apps.
    app.new_window()
        .key_released(key_released)
        .mouse_wheel(mouse_wheel)
        .build()
        .unwrap();
    app.set_loop_mode(LoopMode::Wait);

    // Create the UI.
    let mut ui = app.new_ui().build().unwrap();

    // Generate some ids for our widgets.
    let ids = Ids::new(ui.widget_id_generator());

    // Init our variables
    let scale = 5.0;
    let position = pt2(0.0, 0.0);
    let apply_function = true;
    let resolution = 2.0;
    let x_min = -0.0;
    let x_max = 1.0;
    let y_min = 0.0;
    let y_max = 1.0;
    let parameter_1 = Complex::<f32>::new(1.0, 0.0);
    let parameter_homotopy = 0.0;

    Model {
        ui,
        ids,
        scale,
        position,
        apply_function,
        resolution,
        x_min,
        x_max,
        y_min,
        y_max,
        parameter_1,
        parameter_homotopy,
    }
}

fn key_released(_app: &App, model: &mut Model, key: Key) {
    match key {
        Key::Space => model.apply_function = !model.apply_function,
        Key::R => model.parameter_1 = Complex::new(1.0, 0.0),
        _ => {}
    }
}

fn mouse_wheel(app: &App, model: &mut Model, dt: MouseScrollDelta, _phase: TouchPhase) {
    match dt {
        MouseScrollDelta::PixelDelta(pos) => {
            if app.keys.down.contains(&Key::A) {
                model.x_min -= pos.x as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if app.keys.down.contains(&Key::D) {
                model.x_max -= pos.x as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if app.keys.down.contains(&Key::W) {
                model.y_max -= pos.y as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if app.keys.down.contains(&Key::S) {
                model.y_min -= pos.y as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if app.keys.down.contains(&Key::LShift) {
                model.scale -= pos.y as f32 / 10.0
            }
            if app.keys.down.contains(&Key::LControl) {
                model.resolution -= pos.y as f32 / 20.0
            }
            if app.keys.down.is_empty() {
                model.position -= pt2(pos.x as f32, pos.y as f32) / model.scale.exp() * 10.0
            }
        }
        _ => {}
    }
}

fn update(_app: &App, model: &mut Model, _update: Update) {
    // Calling `set_widgets` allows us to instantiate some widgets.
    let ui = &mut model.ui.set_widgets();

    fn slider(val: f32, min: f32, max: f32) -> widget::Slider<'static, f32> {
        widget::Slider::new(val, min, max)
            .w_h(200.0, 30.0)
            .label_font_size(15)
            .rgb(0.3, 0.3, 0.3)
            .label_rgb(1.0, 1.0, 1.0)
            .border(0.0)
    }

    for value in slider(model.scale, 10.0, 500.0)
        .top_left_with_margin(10.0)
        .label("Scale")
        .set(model.ids.scale, ui)
    {
        model.scale = value;
    }

    for value in widget::Toggle::new(model.apply_function)
        .w_h(200.0, 30.0)
        .label_font_size(15)
        .rgb(0.3, 0.3, 0.3)
        .label_rgb(1.0, 1.0, 1.0)
        .border(0.0)
        .down(10.0)
        .label("Apply function")
        .set(model.ids.apply_function, ui)
    {
        model.apply_function = value;
    }

    for value in slider(model.resolution, 0.0, 5.0)
        .down(10.0)
        .label("Resolution")
        .set(model.ids.resolution, ui)
    {
        model.resolution = value;
    }

    for value in slider(model.x_min, -15.0, 15.0)
        .down(10.0)
        .label("x_min")
        .set(model.ids.x_min, ui)
    {
        model.x_min = value;
    }

    for value in slider(model.x_max, -15.0, 15.0)
        .down(10.0)
        .label("x_max")
        .set(model.ids.x_max, ui)
    {
        model.x_max = value;
    }

    for value in slider(model.y_min, -15.0, 15.0)
        .down(10.0)
        .label("y_min")
        .set(model.ids.y_min, ui)
    {
        model.y_min = value;
    }

    for value in slider(model.y_max, -15.0, 15.0)
        .down(10.0)
        .label("y_max")
        .set(model.ids.y_max, ui)
    {
        model.y_max = value;
    }
    for (x, y) in widget::XYPad::new(
        model.parameter_1.re,
        -3.0,
        3.0,
        model.parameter_1.im,
        -3.0,
        3.0,
    )
    .down(10.0)
    .w_h(200.0, 200.0)
    .label("Parameter 1")
    .label_font_size(15)
    .rgb(0.3, 0.3, 0.3)
    .label_rgb(1.0, 1.0, 1.0)
    .border(0.0)
    .set(model.ids.parameter_1, ui)
    {
        model.parameter_1 = Complex::<f32>::new(x, y);
    }
    for value in slider(model.parameter_homotopy, 0.0, 1.0)
        .down(10.0)
        .label("homotopy parameter")
        .set(model.ids.parameter_homotopy, ui)
    {
        model.parameter_homotopy = value;
    }
}

fn coordinate_lines(model: &Model) -> Vec<Vec<Point2>> {
    vec![
        vec![
            to_screen_coords(-1.0, 0.0, model),
            to_screen_coords(1.0, 0.0, model),
        ],
        vec![
            to_screen_coords(0.0, -1.0, model),
            to_screen_coords(0.0, 1.0, model),
        ],
    ]
}

fn to_screen_coords(x: f32, y: f32, model: &Model) -> Point2 {
    (pt2(x, y) + model.position) * model.scale.exp()
}

// Draw the state of your `Model` into the given `Frame` here.
fn view(app: &App, model: &Model, frame: Frame) {
    let (points, line_structure) = create_gridlines(
        4.0,
        model.resolution,
        model.x_min,
        model.x_max,
        model.y_min,
        model.y_max,
    );
    let points = points.map(|z| {
        if model.apply_function {
            // change this to visualize a different function
            (1.0 - z) / (1.0 + z) * model.parameter_1
        } else {
            z
        }
    });
    let mut points = points.map(|z| to_screen_coords(z.re, z.im, model));

    // Begin drawing
    let draw = app.draw();

    draw.background().rgb(0.02, 0.02, 0.02);

    for coord_line in coordinate_lines(&model) {
        draw.arrow()
            .weight(1.0)
            .head_length(0.1 * model.scale.exp())
            .head_width(0.05 * model.scale.exp())
            .color(srgba(1.0, 1.0, 1.0, 1.0))
            .start(coord_line[0])
            .end(coord_line[1]);
    }

    for line_len in line_structure {
        let line: Vec<_> = points.by_ref().take(line_len).collect();
        draw.polyline()
            .weight(2.0)
            .color(srgba(1.0, 1.0, 1.0, 1.0))
            .points(line);
    }

    // Write the result of our drawing to the window's frame.
    draw.to_frame(app, &frame).unwrap();

    // Draw the state of the `Ui` to the frame.
    model.ui.draw_to_frame(app, &frame).unwrap();
}
