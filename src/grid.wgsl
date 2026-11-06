// Draws the mapped grid. Every instance is one straight segment of a grid line
// in the source plane; the vertex shader maps both ends through `g` and expands
// the result into a screen-space quad. No vertex data is read: the quad corner
// comes from the vertex index, the segment from the instance index.

#import bevy_pbr::mesh_view_bindings::view

struct GridParams {
    // The complex parameter p.
    parameter: vec2<f32>,
    // Offset of the plane on screen, in plane units.
    offset: vec2<f32>,
    // Grid layout, see `GridLayout` in src/lib.rs.
    grid_min: vec2<f32>,
    grid_max: vec2<f32>,
    spacing: f32,
    h_lines: u32,
    h_segments: u32,
    v_segments: u32,
    pixels_per_unit: f32,
    // 0 shows f_start, 1 shows f.
    blend: f32,
    // Not 0: peeking at the source grid, which is z itself whatever f_start is.
    peek: u32,
    line_width: f32,
    coloring: u32,
    // Stretch factors beyond 2^±stretch_octaves get the extreme color.
    stretch_octaves: f32,
    // The whole grid domain, for Source coloring. The laid-out grid
    // (grid_min/grid_max) can be just the visible part of it.
    domain_min: vec2<f32>,
    domain_max: vec2<f32>,
    // Half the window size, in screen points. The origin is the window centre.
    half_view: vec2<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: GridParams;

// The two ends of the homotopy, `f_start(z, p)` and `f(z, p)`, with
// `START_IS_IDENTITY`, and the complex helpers they use (`cmul`, `cexp`, ...)
// are appended at runtime: see `Homotopy::to_wgsl` in src/expr.rs,
// src/complex.wgsl and `grid_shader_source` in src/main.rs. For a whole
// function f_start is the identity; for one stage, the stage before it.

// ---------------------------------------------------------------------------

// The map on screen: blends from f_start to f.
fn g(z: vec2<f32>) -> vec2<f32> {
    if params.peek != 0u {
        return z;
    }
    let start = f_start(z, params.parameter);
    return start + params.blend * (f(z, params.parameter) - start);
}

// f'(z) by central differences. f is holomorphic, so any direction works.
fn f_prime(z: vec2<f32>) -> vec2<f32> {
    let h = 1e-3 * max(1.0, length(z));
    let p = params.parameter;
    return (f(z + vec2(h, 0.0), p) - f(z - vec2(h, 0.0), p)) / (2.0 * h);
}

// The same for f_start.
fn f_start_prime(z: vec2<f32>) -> vec2<f32> {
    if START_IS_IDENTITY {
        return ONE;
    }
    let h = 1e-3 * max(1.0, length(z));
    let p = params.parameter;
    return (f_start(z + vec2(h, 0.0), p) - f_start(z - vec2(h, 0.0), p)) / (2.0 * h);
}

// g'(z) from f'(z): g blends linearly, so its derivative does too.
fn g_prime(z: vec2<f32>, df: vec2<f32>) -> vec2<f32> {
    if params.peek != 0u {
        return ONE;
    }
    let start = f_start_prime(z);
    return start + params.blend * (df - start);
}

// The grid line that segment i lies on, and where on it. Point j of the line,
// `point(line, j)`, is where its segment j starts; together they mirror
// `GridLayout::segment` in src/lib.rs.
struct Line {
    origin: vec2<f32>,
    // One segment along the line.
    step: vec2<f32>,
    // Segments on the line.
    n: u32,
    // Which of them segment i is.
    k: u32,
    horizontal: bool,
};

fn line_of(i: u32) -> Line {
    var l: Line;
    let h_total = params.h_lines * params.h_segments;
    if i < h_total {
        let line = i / params.h_segments;
        l.k = i % params.h_segments;
        l.n = params.h_segments;
        l.origin = vec2(params.grid_min.x, params.grid_min.y + f32(line) * params.spacing);
        l.step = vec2((params.grid_max.x - params.grid_min.x) / f32(params.h_segments), 0.0);
        l.horizontal = true;
    } else {
        let j = i - h_total;
        let line = j / params.v_segments;
        l.k = j % params.v_segments;
        l.n = params.v_segments;
        l.origin = vec2(params.grid_min.x + f32(line) * params.spacing, params.grid_min.y);
        l.step = vec2(0.0, (params.grid_max.y - params.grid_min.y) / f32(params.v_segments));
        l.horizontal = false;
    }
    return l;
}

fn point(l: Line, j: u32) -> vec2<f32> {
    return l.origin + f32(min(j, l.n)) * l.step;
}

// Where point j of the line lands on screen.
fn mapped(l: Line, j: u32) -> vec2<f32> {
    return to_screen(g(point(l, j)));
}

// ---------------------------------------------------------------------------
// Level of detail. Where the map shrinks the grid, many segments land inside a
// pixel, and drawing each as its own quad costs far more than the picture
// needs (Escher's spiral packs ~1M of them into a few hundred pixels). So runs
// of 2^L segments that together span less than MERGE_BELOW pixels are drawn as
// a single chord.
//
// A run is aligned: it starts at a segment index divisible by 2^L. Whether it
// is short enough is estimated from its first segment alone, as that
// segment's chord times 2^L. That makes the verdict a property of the run, so
// every segment reaches the same verdict about it, and the estimate follows
// the arc rather than the run's own chord, which is ~0 when the run loops
// around. The run that gets drawn for a segment is the coarsest short run
// containing it whose parent run is not short. Every segment lies in exactly
// such a run (a single segment always counts as short enough), so no gaps.

const MERGE_BELOW: f32 = 1.0;
const MAX_MERGE_LEVEL: u32 = 10u;

// Largest L <= MAX_MERGE_LEVEL with chord * 2^L <= MERGE_BELOW, or 0.
fn short_level(chord: f32) -> u32 {
    let levels = floor(log2(MERGE_BELOW / max(chord, 1e-30)));
    return u32(clamp(levels, 0.0, f32(MAX_MERGE_LEVEL)));
}

// The level of the run that segment k draws, given its own chord on screen,
// or -1 if a run it lies in is drawn by an earlier segment.
fn merge_level(l: Line, chord: f32) -> i32 {
    // k starts runs up to this level.
    let top = min(countTrailingZeros(l.k), MAX_MERGE_LEVEL);
    let own = short_level(chord);
    if own < top {
        return i32(own);
    }
    if top == MAX_MERGE_LEVEL {
        return i32(top);
    }
    // The run at level `top` is short; drawn here unless its parent run, which
    // starts 2^top segments earlier, is short too.
    let p = l.k - (1u << top);
    let parent_chord = length(mapped(l, p + 1u) - mapped(l, p));
    if parent_chord * f32(2u << top) <= MERGE_BELOW {
        return -1;
    }
    return i32(top);
}

// ---------------------------------------------------------------------------
// Color schemes, in sRGB. Mirrored in src/main.rs for the legend.

const COLORING_WHITE: u32 = 0u;
const COLORING_SOURCE: u32 = 1u;
const COLORING_PHASE: u32 = 2u;
const COLORING_STRETCH: u32 = 3u;
const COLORING_ROTATION: u32 = 4u;
const TAU: f32 = 6.28318530718;

// t in [0, 1], wraps around.
fn cyclic(t: f32) -> vec3<f32> {
    return 0.5 + 0.5 * cos(TAU * (t + vec3(0.0, 1.0 / 3.0, 2.0 / 3.0)));
}

// t in [-1, 1]: blue for shrinking, light grey for 1, orange for growing.
fn diverging(t: f32) -> vec3<f32> {
    let neutral = vec3(0.85, 0.85, 0.85);
    if t < 0.0 {
        return mix(neutral, vec3(0.2, 0.5, 1.0), -t);
    }
    return mix(neutral, vec3(1.0, 0.45, 0.1), t);
}

// Horizontal lines are warm (red at the bottom to yellow at the top), vertical
// lines are cool (cyan on the left to violet on the right).
fn source(horizontal: bool, s: f32) -> vec3<f32> {
    if horizontal {
        return mix(vec3(1.0, 0.3, 0.2), vec3(1.0, 0.9, 0.3), s);
    }
    return mix(vec3(0.2, 0.8, 1.0), vec3(0.7, 0.4, 1.0), s);
}

// `df` is f'(z), not the blended g'(z), so Stretch and Rotation keep showing
// f (for a stage: the whole composition up to it) while peeking at the source
// grid, like Phase does.
fn line_color(z: vec2<f32>, horizontal: bool, df: vec2<f32>) -> vec3<f32> {
    switch params.coloring {
        case COLORING_SOURCE: {
            let size = max(params.domain_max - params.domain_min, vec2(1e-6));
            let s = clamp((z - params.domain_min) / size, vec2(0.0), vec2(1.0));
            return source(horizontal, select(s.x, s.y, horizontal));
        }
        case COLORING_PHASE: {
            return cyclic(atan2(z.y, z.x) / TAU);
        }
        case COLORING_STRETCH: {
            return diverging(clamp(log2(length(df)) / params.stretch_octaves, -1.0, 1.0));
        }
        case COLORING_ROTATION: {
            return cyclic(atan2(df.y, df.x) / TAU);
        }
        default: {
            return vec3(1.0);
        }
    }
}

// ---------------------------------------------------------------------------

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

// Lines this far from the centre (in pixels) are treated as running off to infinity.
const MAX_SCREEN_COORD: f32 = 1e5;
// Outside the depth range, so the triangle is clipped away.
const CULLED = vec4<f32>(0.0, 0.0, 2.0, 1.0);

fn to_screen(w: vec2<f32>) -> vec2<f32> {
    return (w + params.offset) * params.pixels_per_unit;
}

// Whether the bounding box of the chord from a to b, grown by `margin`,
// overlaps the window.
fn near_view(a: vec2<f32>, b: vec2<f32>, margin: f32) -> bool {
    return all(max(a, b) > -params.half_view - margin)
        && all(min(a, b) < params.half_view + margin);
}

fn finite(a: vec2<f32>, b: vec2<f32>) -> bool {
    return all(abs(a) < vec2(MAX_SCREEN_COORD)) && all(abs(b) < vec2(MAX_SCREEN_COORD));
}

@vertex
fn vertex(
    @builtin(vertex_index) vertex_index: u32,
    @builtin(instance_index) instance_index: u32,
) -> VertexOutput {
    var out: VertexOutput;
    // Corners 0..3 of the quad: bit 1 picks the end, bit 0 the side of the line.
    let corner = vertex_index % 4u;
    let at_end = corner >= 2u;
    let side = select(-1.0, 1.0, (corner & 1u) == 1u);

    let l = line_of(instance_index);
    let a = mapped(l, l.k);
    let own_end = mapped(l, l.k + 1u);

    // Drop segments outside the window before the costly part below. The
    // margin leaves room for a merged run starting here, which spans at most
    // about MERGE_BELOW pixels. NaN ends fail the comparisons and are dropped
    // too. Every decision here uses only values of the segment or its runs, so
    // all four corners of the quad agree.
    let reach = 0.5 * params.line_width;
    if !(near_view(a, own_end, reach + 2.0 * MERGE_BELOW) && finite(a, own_end)) {
        out.position = CULLED;
        return out;
    }

    let level = merge_level(l, length(own_end - a));
    if level < 0 {
        out.position = CULLED;
        return out;
    }
    let end = min(l.k + (1u << u32(level)), l.n);
    var b = own_end;
    if level > 0 {
        b = mapped(l, end);
    }
    // The quad is the straight chord from a to b, widened by the line width,
    // so its bounding box decides exactly. Far-off ends count as infinity even
    // if the chord crosses the window.
    if !(near_view(a, b, reach) && finite(a, b)) {
        out.position = CULLED;
        return out;
    }

    let start_z = point(l, l.k);
    let end_z = point(l, end);
    let df_start = f_prime(start_z);
    let df_end = f_prime(end_z);
    let z = select(start_z, end_z, at_end);
    let df = select(df_start, df_end, at_end);

    // Drop segments that jump across a pole instead of following the curve.
    // Across a pole the image runs off to
    // infinity and back in from the other side, so the chord points against
    // the curve's direction at the ends, and is often much longer than the
    // derivative predicts. Both ends are checked, never the midpoint: a
    // midpoint near the pole has a huge derivative that excuses any chord.
    // Every corner of the quad must reach the same verdict, so this uses only
    // per-segment values.
    let step = (end_z - start_z) * params.pixels_per_unit;
    let tangent_start = cmul(g_prime(start_z, df_start), step);
    let tangent_end = cmul(g_prime(end_z, df_end), step);
    let chord_vec = b - a;
    let chord = length(chord_vec);
    let expected = max(length(tangent_start), length(tangent_end));
    let forward = dot(chord_vec, tangent_start) >= 0.0 && dot(chord_vec, tangent_end) >= 0.0;
    // Tiny chords can point backwards at a critical point (f' = 0) without
    // jumping anywhere; keep those so no gap opens there.
    let follows_curve = (forward || chord < 4.0 * params.line_width)
        && chord < 4.0 * expected + params.line_width;
    if !follows_curve {
        out.position = CULLED;
        return out;
    }

    let dir = select(vec2(1.0, 0.0), (b - a) / chord, chord > 1e-6);
    let normal = vec2(-dir.y, dir.x);
    // Extend each end by half the width so neighbouring segments overlap at bends.
    let along = select(-1.0, 1.0, at_end);
    let pos = select(a, b, at_end)
        + 0.5 * params.line_width * (side * normal + along * dir);

    out.position = view.clip_from_world * vec4(pos, 0.0, 1.0);
    out.color = vec4(pow(line_color(z, l.horizontal, df), vec3(2.2)), 1.0);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
