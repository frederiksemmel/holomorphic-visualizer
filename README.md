# Conformal Map Visualizer

Built with [nannou](https://nannou.cc) 0.20 (on Bevy) and its egui integration.

## Examples

Pick one from the **Example** menu in the control window, or press `e` to step
through them. Each brings its own grid and parameter, and fills in its formula,
which you can then edit (see [Usage](#usage)). The screenshots use the
Source coloring (`2`): horizontal lines warm, vertical lines cool.

The basic one is a Möbius transformation, p · (1 − z)/(1 + z), applied to the
unit square. It should be called a Mickey Mouse function, as every Möbius
transformation contains a Mickey Mouse (unproven conjecture).

![Möbius transformation](examples/mobius.png)

The coolest one is M. C. Escher's
[Print Gallery](https://en.wikipedia.org/wiki/Print_Gallery_(M._C._Escher))
(1956): a young man in a gallery looks at a print of a town that contains the
gallery he is standing in. De Smit and Lenstra worked out the math in
[The Mathematical Structure of Escher's Print Gallery](https://pub.math.leidenuniv.nl/~smitbde/papers/2003-de_smit-lenstra-escher.pdf)
(Notices of the AMS, 2003). Escher started from a straight picture that
contains itself, scaled down by 256, and curled it with

$$w \mapsto w^{1/\alpha}, \qquad \alpha = \frac{2\pi i + \log 256}{2\pi i} \approx 1 - 0.8825i,$$

so that one turn around the centre zooms in by 256. His picture contains a copy
of itself shrunk by 22.58 and turned by 157.6°, so it zooms in forever while
circling the origin. The grid here is the straight picture in log coordinates,
w = exp(z): vertical lines are circles around its centre, horizontal lines are
rays. The map draws them the way Escher curled them:

![Escher's Print Gallery map](examples/escher.png)

Zoom in on the centre (`shift + scroll`) and the spiral keeps going:

![Escher's Print Gallery map, zoomed in](examples/escher-zoom.png)

z³ applied to a tall rectangle just right of 0 (the Power example, z^p with
p = 3) wraps the band around the origin:

![z^3](examples/power.png)

The others: exp(p · z), p · sin z and the Joukowski map z + p / z.

![exp(z)](examples/exponential.png)
![sin z](examples/sine.png)
![Joukowski map](examples/joukowski.png)

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
* Type any function into the `f(z) =` field, for example `z^2 + p`,
  `p sin(z) / z` or `exp(1/z)`. The picture follows as you type; while a
  formula has an error, the message says where and the last working one stays
  on screen. `Esc` goes back to the example.
    * `z` is the point, `p` the parameter set with the XY pad, plus `i`, `pi`
      (or `π`) and `e`.
    * `+ - * / ^` with the usual precedence; `^` binds tightest, to the right.
      Writing things next to each other multiplies: `2pi i z`, `p sin(z)`.
    * Functions, with parentheses: `exp`, `log` (or `ln`), `sqrt`, `sin`,
      `cos`, `tan`, `sinh`, `cosh`, `tanh`. `log`, `sqrt` and non-integer
      powers use the principal branch, cut along the negative real axis.
      Integer powers like `z^3` or `z^-2` are multiplied out and have no cut.
* Dynamic function eval: the grid is mapped on the GPU, so the formula is
  never evaluated on the CPU. `src/expr.rs` parses it and translates it into
  WGSL (`p sin(z)` becomes `cmul(p, csin(z))`), and the app rebuilds the grid
  shader from `src/grid.wgsl`, the complex helpers in `src/complex.wgsl` and
  the generated `f`, which Bevy recompiles. Only formulas that parse are sent
  to the GPU, and the tests compile the code generated for a range of formulas
  with naga.
* Color modes (`c` cycles them, `1`–`5` pick one):
    * **White**: plain lines.
    * **Source**: horizontal lines warm, vertical lines cool, shaded by
      position, so you can follow each line of the grid to its image.
    * **Phase**: the hue of each point is the angle `arg z` of where it came
      from. Peek at the source grid (hold `x`) to see the color wheel, then let
      go to see where each color went.
    * **Stretch**: `|f'(z)|`, how much the map scales lengths near each point.
      Blue shrinks, grey keeps, orange grows.
    * **Rotation**: `arg f'(z)`, how far the map turns directions near each
      point, as a color wheel.

    A conformal map acts near each point like multiplying by `f'(z)`, so Stretch
    and Rotation together show everything the map does locally. The grid
    shows that right angles are kept.
* Controls. Most are a key held down while you scroll (mouse wheel or
  touchpad) or just move the mouse:
    * drag or scroll to pan, `shift + scroll` to zoom around the cursor, `h` to
      reset the view
    * `ctrl + scroll` to make the grid finer or coarser
    * hold `w` / `a` / `s` / `d` and scroll or move the mouse to drag the top /
      left / bottom / right edge of the grid. Hold two opposite ones (`a + d`,
      `w + s`) to slide the grid along, all four to move it
    * on a laptop touchpad, holding a letter key may switch the touchpad off
      (libinput's disable-while-typing). Press `ctrl` first, then the key:
      `ctrl + a + scroll` works the same, and the touchpad stays on. This goes
      for `p` and `f` below too
    * hold `p` and scroll or move the mouse, or right-drag the canvas, to move
      the parameter `p`; `r` resets it to the example's value
    * hold `f` and scroll or move the mouse to blend from the identity to `f`
      (the homotopy)
    * `e` steps to the next example
    * `space` toggles applying the function; hold `x` or the "Hold to show
      source" button to peek at the source grid
    * the control window has sliders for everything, with these shortcuts
      written underneath
