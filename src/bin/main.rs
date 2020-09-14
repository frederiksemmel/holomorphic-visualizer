use nannou::prelude::*;
use nannou::ui::prelude::*;
extern crate hologrid;
use hologrid::create_gridlines;

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
    left_bound_active: bool,
    right_bound_active: bool,
    top_bound_active: bool,
    bottom_bound_active: bool,
    zoom_active: bool,
    resolution_active: bool,
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
    }
}

fn model(app: &App) -> Model {
    // Set the loop mode to wait for events, an energy-efficient option for pure-GUI apps.
    app.new_window()
        .mouse_wheel(mouse_wheel)
        .key_pressed(key_pressed)
        .key_released(key_released)
        .build()
        .unwrap();
    app.set_loop_mode(LoopMode::Wait);

    // Create the UI.
    let mut ui = app.new_ui().build().unwrap();

    // Generate some ids for our widgets.
    let ids = Ids::new(ui.widget_id_generator());

    // Init our variables
    let scale = 100.0;
    let position = pt2(0.0, 0.0);
    let apply_function = true;
    let resolution = 2.0;
    let x_min = -5.0;
    let x_max = 5.0;
    let y_min = -5.0;
    let y_max = 5.0;
    let (
        left_bound_active,
        right_bound_active,
        top_bound_active,
        bottom_bound_active,
        zoom_active,
        resolution_active,
    ) = (false, false, false, false, false, false);

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
        right_bound_active,
        left_bound_active,
        top_bound_active,
        bottom_bound_active,
        zoom_active,
        resolution_active
    }
}

fn key_pressed(_app: &App, model: &mut Model, key: Key) {
    match key {
        Key::A => model.left_bound_active = true,
        Key::D => model.right_bound_active = true,
        Key::W => model.top_bound_active = true,
        Key::S => model.bottom_bound_active = true,
        Key::LShift => model.zoom_active = true,
        Key::Space => model.apply_function = !model.apply_function,
        Key::LControl => model.resolution_active = true,
        _ => {}
    }
}

fn key_released(_app: &App, model: &mut Model, key: Key) {
    match key {
        Key::A => model.left_bound_active = false,
        Key::D => model.right_bound_active = false,
        Key::W => model.top_bound_active = false,
        Key::S => model.bottom_bound_active = false,
        Key::LShift => model.zoom_active = false,
        Key::LControl => model.resolution_active = false,
        _ => {}
    }
}

fn mouse_wheel(_app: &App, model: &mut Model, dt: MouseScrollDelta, _phase: TouchPhase) {
    match dt {
        MouseScrollDelta::PixelDelta(pos) => {
            if model.left_bound_active {
                model.x_min -= pos.x as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if model.right_bound_active {
                model.x_max -= pos.x as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if model.top_bound_active {
                model.y_max -= pos.y as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if model.bottom_bound_active {
                model.y_min -= pos.y as f32 / pow(2.0, model.resolution as usize) as f32 / 2.0
            }
            if model.zoom_active {
                model.scale += pos.y as f32
            }
            if model.resolution_active {
                model.resolution += pos.y as f32 / 20.0
            }
            if !model.left_bound_active
                && !model.right_bound_active
                && !model.top_bound_active
                && !model.bottom_bound_active
                && !model.zoom_active
                && !model.resolution_active
            {
                model.position -= pt2(pos.x as f32 * 8.0, pos.y as f32 * 8.0)
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
}

// Draw the state of your `Model` into the given `Frame` here.
fn view(app: &App, model: &Model, frame: Frame) {
    let (points, line_structure) = create_gridlines(
        model.resolution,
        model.x_min,
        model.x_max,
        model.y_min,
        model.y_max,
    );
    let points = points.map(|z| if model.apply_function { z * z } else { z });
    let mut points = points.map(|z| pt2(z.re, z.im) * model.scale + model.position);

    // Begin drawing
    let draw = app.draw();

    draw.background().rgb(0.02, 0.02, 0.02);
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
