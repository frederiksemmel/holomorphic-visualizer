# Conformal Map Visualizer

**[Open it in your browser →](https://frederiksemmel.github.io/holomorphic-visualizer/)**

![A grid of the plane bending under exp(z) and back](web/preview.gif)

Type any complex function f(z) and watch it bend a grid of the plane, live on
the GPU, in the spirit of 3Blue1Brown's animations. Slide from the identity to
f, drag the parameter p around, and color the lines by where they came from,
by how much f stretches them, or by how far it turns them.

The URL always holds what you're looking at, so you can share a view by
sharing the link (there's also a **Copy link** button). For example:
[Escher's Print Gallery](https://frederiksemmel.github.io/holomorphic-visualizer/#ex=escher),
[exp(1/z) near its essential singularity](https://frederiksemmel.github.io/holomorphic-visualizer/#f=exp%281/z%29&grid=-1,1,-1,1&res=4&color=source&view=0,0,300),
[the Joukowski map stretching a disk](https://frederiksemmel.github.io/holomorphic-visualizer/#ex=joukowski&color=stretch).

It needs WebGPU: Chrome or Edge, Safari 26+, or Firefox 141+ on Windows. On
Linux, Chrome needs `chrome://flags/#enable-unsafe-webgpu`. Without it, run
the [desktop app](#development).

## Examples

Pick one from the **Example** menu in the control window, or press `e` to step
through them. Each brings its own grid and parameter, and fills in its formula,
which you can then edit (see [Usage](#usage)). The screenshots use the
default Phase coloring (`3`): every point keeps the hue of its angle arg z in
the source grid, so you can see where each direction went.

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
rays. The map draws them the way Escher curled them.

The example zooms by e⁶ ≈ 403 per turn instead of 256:

$$f(z) = \exp\left(\frac{2\pi i\, z}{2\pi i + \log(p\, e^6)}\right), \qquad p = 1.$$

f repeats itself after z moves by 2πi + log(p e⁶), so a circle that leaves one
turn comes back log(p e⁶) further in. With p = 1 that is exactly 6, a whole
number of grid spacings at every resolution, so each circle meets up with
another one. Escher's 256 is p = 256/e⁶ ≈ 0.635, where log 256 ≈ 5.545 and
the circles miss each other.

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

Built with [nannou](https://nannou.cc) 0.20 (on Bevy) and its egui integration.
The dev environment is managed with [devenv](https://devenv.sh); enter it with
`devenv shell`. It provides the Rust toolchain, fontconfig for building, and the
runtime libraries (Vulkan/GL, Wayland, X11) the window needs.

The first build compiles Bevy (~590 crates), so expect a few minutes.

```sh
cargo run --release   # run the visualizer
cargo test            # unit tests (also: `devenv test`)
```

The desktop app takes a link, or just its `#…` part, as its argument:
`cargo run --release -- '#ex=escher&color=source'`. Its **Copy link** button
copies a link to the browser version.

Inside the shell, cargo is limited to 90% of the CPU cores (`CARGO_BUILD_JOBS`).

### Web

```sh
build-web    # build the browser version into web/dist
serve-web    # build it and serve it on http://127.0.0.1:8080
```

CI (`.github/workflows/web.yml`) builds it for every pull request and publishes
it to GitHub Pages from `master`. nannou is vendored in `vendor/nannou` with
fixes for the web build; see the note in `Cargo.toml`.

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
    * **Phase** (the default): the hue of each point is the angle `arg z` of
      where it came from. Peek at the source grid (hold `x`) to see the color wheel, then let
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
    * `m` (or the Animate box) plays the blend back and forth, as it does when
      the visualizer opens or you pick an example; taking the homotopy in hand
      stops it
    * **Copy link** copies a link to exactly this view. The page URL always
      shows it as well: the example, the formula, `p`, the grid, the coloring,
      the view and the homotopy, written as what differs from the example.
      Editing the `#…` part of the URL by hand works too
    * `e` steps to the next example
    * `space` toggles applying the function; hold `x` or the "Hold to show
      source" button to peek at the source grid
    * the control window has sliders for everything, with these shortcuts
      written underneath
