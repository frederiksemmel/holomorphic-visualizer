use nannou::prelude::*;
use nannou::ui::prelude::*;

fn main() {
    nannou::app(model).update(update).simple_window(view).run();
}

struct Model {
    ui: Ui,
    resolution: usize,
    scale: f32,
    rotation: f32,
    color: Rgb,
    position: Point2,
}

fn model(app: &App) -> Model {
    // Set the loop mode to wait for events, an energy-efficient option for pure-GUI apps.
    app.set_loop_mode(LoopMode::Wait);

    // Create the UI.
    let mut ui = app.new_ui().build().unwrap();

    // Init our variables
    let resolution = 6;
    let scale = 200.0;
    let rotation = 0.0;
    let position = pt2(0.0, 0.0);
    let color = rgb(1.0, 0.0, 1.0);

    Model {
        ui,
        resolution,
        scale,
        rotation,
        position,
        color,
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

    for value in slider(model.resolution as f32, 3.0, 15.0)
        .top_left_with_margin(20.0)
        .label("Resolution")
    {
        model.resolution = value as usize;
    }

    for value in slider(model.scale, 10.0, 500.0)
        .down(10.0)
        .label("Scale")
    {
        model.scale = value;
    }

    for value in slider(model.rotation, -PI, PI)
        .down(10.0)
        .label("Rotation")
    {
        model.rotation = value;
    }

    for _click in widget::Button::new()
        .down(10.0)
        .w_h(200.0, 60.0)
        .label("Random Color")
        .label_font_size(15)
        .rgb(0.3, 0.3, 0.3)
        .label_rgb(1.0, 1.0, 1.0)
        .border(0.0)
    {
        model.color = rgb(random(), random(), random());
    }

    for (x, y) in widget::XYPad::new(
        model.position.x,
        -200.0,
        200.0,
        model.position.y,
        -200.0,
        200.0,
    )
    .down(10.0)
    .w_h(200.0, 200.0)
    .label("Position")
    .label_font_size(15)
    .rgb(0.3, 0.3, 0.3)
    .label_rgb(1.0, 1.0, 1.0)
    .border(0.0)
    {
        model.position = Point2::new(x, y);
    }
}

// Draw the state of your `Model` into the given `Frame` here.
fn view(app: &App, model: &Model, frame: Frame) {
    // Begin drawing
    let draw = app.draw();

    draw.background().rgb(0.02, 0.02, 0.02);

    draw.ellipse()
        .xy(model.position)
        .radius(model.scale)
        .resolution(model.resolution)
        .rotate(model.rotation)
        .color(model.color);

    // Write the result of our drawing to the window's frame.
    draw.to_frame(app, &frame).unwrap();

    // Draw the state of the `Ui` to the frame.
    model.ui.draw_to_frame(app, &frame).unwrap();
}

