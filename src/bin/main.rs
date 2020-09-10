use nannou::prelude::*;
use nannou::ui::prelude::*;
extern crate hologrid;
use hologrid::create_gridlines;

fn main() {
    nannou::app(model).update(update).simple_window(view).run();
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
}

widget_ids! {
    struct Ids {
        scale,
        position,
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
    app.set_loop_mode(LoopMode::Wait);

    // Create the UI.
    let mut ui = app.new_ui().build().unwrap();

    // Generate some ids for our widgets.
    let ids = Ids::new(ui.widget_id_generator());

    // Init our variables
    let scale = 100.0;
    let position = pt2(0.0, 0.0);
    let apply_function = true;
    let resolution = 0.0;
    let x_min = -5.0;
    let x_max = 5.0;
    let y_min = -5.0;
    let y_max = 5.0;

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

    for (x, y) in widget::XYPad::new(
        model.position.x,
        -1000.0,
        1000.0,
        model.position.y,
        -1000.0,
        1000.0,
    )
    .down(10.0)
    .w_h(200.0, 200.0)
    .label("Position")
    .label_font_size(15)
    .rgb(0.3, 0.3, 0.3)
    .label_rgb(1.0, 1.0, 1.0)
    .border(0.0)
    .set(model.ids.position, ui)
    {
        model.position = Point2::new(x, y);
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

    for value in slider(model.resolution, -2.0, 4.0)
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

fn apply_mapping<F>(f: F, lines: Vec<Vec<Point2>>) -> Vec<Vec<Point2>>
where
    F: Fn(Point2) -> Point2,
{
    lines
        .into_iter()
        .map(|line| line.into_iter().map(|z| f(z)).collect())
        .collect()
}

fn to_screen_coord(lines: Vec<Vec<Point2>>, model: &Model) -> Vec<Vec<Point2>> {
    lines
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|z| z * model.scale + model.position)
                .collect()
        })
        .collect()
}

// Draw the state of your `Model` into the given `Frame` here.
fn view(app: &App, model: &Model, frame: Frame) {
    let mut lines = create_gridlines(
        model.resolution,
        model.x_min,
        model.x_max,
        model.y_min,
        model.y_max,
    );
    if model.apply_function {
        lines = apply_mapping(|z| pt2(z[0]*z[0] - z[1]*z[1], 2.0*z[0]*z[1]), lines);
        //lines = apply_mapping(
        //    |z| {
        //        pt2(
        //            z[0] * z[0] * z[0] - 3.0 * z[0] * z[1] * z[1],
        //            3.0 * z[0] * z[0] * z[1] - z[1] * z[1] * z[1],
        //        )
        //   },
        //    lines,
        //);
    }
    // Begin drawing
    let draw = app.draw();

    draw.background().rgb(0.02, 0.02, 0.02);

    for line in to_screen_coord(lines, model).into_iter() {
        draw.polyline()
            .weight(model.scale / 50.0)
            .color(srgba(1.0, 1.0, 1.0, 1.0))
            .points(line);
    }

    // Write the result of our drawing to the window's frame.
    draw.to_frame(app, &frame).unwrap();

    // Draw the state of the `Ui` to the frame.
    model.ui.draw_to_frame(app, &frame).unwrap();
}
