# Conformal Map Visualizer

## Examples

The exponential function of course looks like this:
![exp(z)](/examples/exponential.png)

While z^3 looks like this shape when applied to a tall rectangle just right of 0:
![z^3](/examples/z^3.png)

This is a Möbius Transformation, in particular (1-z)/(1+z), but it should be called
a Mickey Mouse function, as every Möbius Transformation contains a Mickey Mouse 
(unproven Conjecture)

![mickey_mouse](/examples/mickey_mouse.png)

## Installation

On Linux and macOS:
* Install the rust toolchain in order to have cargo installed by following
  [this](https://www.rust-lang.org/tools/install) guide.

On Windows you are on your own...

## Usage
* You need to change the function to visualize in the source code, as parsing a function is not implemented. This is done in `src/main.rs` at the bottom in the part:
  ```rust
    let points = points.map(|z| {
        if model.apply_function {
            // change this to visualize a different function
            z.exp()
        } else {
            z
        }
    });
  ```
  You can check out the [docs](https://docs.rs/num/0.4.0/num/complex/struct.Complex.html) for num::complex to see what functions you can build.
* Run `cargo run --release` inside the project directory. You have to use
  the `--release` flag because otherwise you get a memory overflow if you
  increase the resolution too much.
* The controls on a touchpad are:
    * `scroll` to move
    * `shift + scroll` to zoom
    * `ctrl + scroll` increas the grid resolution 
    * `[w|a|s|d] + scroll` to make the grid bigger along these sides.
      Can be pressed at the same time
    * `space` to toggle applying the function
    * `r` to reset the parameter to 1
    * use the parameter control on the left with your mouse to control the parameter

