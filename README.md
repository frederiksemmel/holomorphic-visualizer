# Conformal Map Visualizer

Built with [nannou](https://nannou.cc) 0.20 (on Bevy) and its egui integration.

## Examples

The exponential function of course looks like this:
![exp(z)](/examples/exponential.png)

While z^3 looks like this shape when applied to a tall rectangle just right of 0:
![z^3](/examples/z^3.png)

This is a Möbius Transformation, in particular (1-z)/(1+z), but it should be called
a Mickey Mouse function, as every Möbius Transformation contains a Mickey Mouse 
(unproven Conjecture)

![mickey_mouse](/examples/mickey_mouse.png)

## Development

The dev environment is managed with [devenv](https://devenv.sh); enter it with
`devenv shell`. It provides the Rust toolchain, fontconfig for building, and the
runtime libraries (Vulkan/GL, Wayland, X11) the window needs.

The first build compiles Bevy (~590 crates), so expect a few minutes.

```sh
cargo run --release   # run the visualizer
cargo test            # unit tests (also: `devenv test`)
cargo bench           # criterion benchmarks for grid generation
```

Inside the shell, cargo is limited to 90% of the CPU cores (`CARGO_BUILD_JOBS`).

### Nix package

The app is also exposed as a devenv output (built with crate2nix):

```sh
devenv build outputs.conformal-visualizer      # build locally
build-remote                                   # same, but only on remote builders (nixbuild.net)
```

Both print the store path. The binary is `<path>/bin/conformal-visualizer`.

## Usage
* The function to visualize is set in the source code, since there is no
  function parser. Change `f` at the top of `src/main.rs` (and
  `FUNCTION_LABEL` next to it):
  ```rust
  fn f(z: Complex<f32>, p: Complex<f32>) -> Complex<f32> {
      (1.0 - z) / (1.0 + z) * p
  }
  ```
  `p` is the parameter set with the XY pad. See the
  [num-complex docs](https://docs.rs/num-complex/latest/num_complex/struct.Complex.html)
  for the functions you can use.
* Controls:
    * `scroll` or drag to move
    * `shift + scroll` to zoom
    * `ctrl + scroll` to change the grid resolution
    * `[w|a|s|d] + scroll` to move that edge of the grid. You can hold
      more than one at once
    * `space` to toggle applying the function
    * `r` to reset the parameter to 1
    * the control window has sliders for everything, an XY pad for the parameter,
      and a homotopy slider that blends from the identity (0) to `f` (1)

