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
    // 0 shows the identity, 1 shows f.
    blend: f32,
    line_width: f32,
    coloring: u32,
    // Stretch factors beyond 2^±stretch_octaves get the extreme color.
    stretch_octaves: f32,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> params: GridParams;

// ---------------------------------------------------------------------------
// Complex numbers as vec2(re, im). Add and subtract with + and -.

const ONE = vec2<f32>(1.0, 0.0);
const I = vec2<f32>(0.0, 1.0);

fn cmul(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2(a.x * b.x - a.y * b.y, a.x * b.y + a.y * b.x);
}
fn cdiv(a: vec2<f32>, b: vec2<f32>) -> vec2<f32> {
    return vec2(a.x * b.x + a.y * b.y, a.y * b.x - a.x * b.y) / dot(b, b);
}
fn cexp(z: vec2<f32>) -> vec2<f32> {
    return exp(z.x) * vec2(cos(z.y), sin(z.y));
}
fn clog(z: vec2<f32>) -> vec2<f32> {
    return vec2(log(length(z)), atan2(z.y, z.x));
}
fn cpow(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
    return cexp(cmul(w, clog(z)));
}
fn csin(z: vec2<f32>) -> vec2<f32> {
    return vec2(sin(z.x) * cosh(z.y), cos(z.x) * sinh(z.y));
}
fn ccos(z: vec2<f32>) -> vec2<f32> {
    return vec2(cos(z.x) * cosh(z.y), -sin(z.x) * sinh(z.y));
}

// The function being visualized. Change this to look at a different map, and
// update FUNCTION_LABEL in src/main.rs to match.
fn f(z: vec2<f32>, p: vec2<f32>) -> vec2<f32> {
    return cmul(cdiv(ONE - z, ONE + z), p);
}

// ---------------------------------------------------------------------------

// The map on screen: blends from the identity to f.
fn g(z: vec2<f32>) -> vec2<f32> {
    return z + params.blend * (f(z, params.parameter) - z);
}

// f'(z) by central differences. f is holomorphic, so any direction works.
fn f_prime(z: vec2<f32>) -> vec2<f32> {
    let h = 1e-3 * max(1.0, length(z));
    let p = params.parameter;
    return (f(z + vec2(h, 0.0), p) - f(z - vec2(h, 0.0), p)) / (2.0 * h);
}

// g'(z) from f'(z): g blends linearly, so its derivative does too.
fn g_prime(df: vec2<f32>) -> vec2<f32> {
    return ONE + params.blend * (df - ONE);
}

// Endpoints of segment i; mirrors `GridLayout::segment` in src/lib.rs.
struct Segment {
    start: vec2<f32>,
    end: vec2<f32>,
    horizontal: bool,
};

fn segment(i: u32) -> Segment {
    var s: Segment;
    let h_total = params.h_lines * params.h_segments;
    if i < h_total {
        let line = i / params.h_segments;
        let k = i % params.h_segments;
        let y = params.grid_min.y + f32(line) * params.spacing;
        let step = (params.grid_max.x - params.grid_min.x) / f32(params.h_segments);
        s.start = vec2(params.grid_min.x + f32(k) * step, y);
        s.end = vec2(params.grid_min.x + f32(k + 1u) * step, y);
        s.horizontal = true;
    } else {
        let j = i - h_total;
        let line = j / params.v_segments;
        let k = j % params.v_segments;
        let x = params.grid_min.x + f32(line) * params.spacing;
        let step = (params.grid_max.y - params.grid_min.y) / f32(params.v_segments);
        s.start = vec2(x, params.grid_min.y + f32(k) * step);
        s.end = vec2(x, params.grid_min.y + f32(k + 1u) * step);
        s.horizontal = false;
    }
    return s;
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
// f while peeking at the source grid, like Phase does.
fn line_color(z: vec2<f32>, horizontal: bool, df: vec2<f32>) -> vec3<f32> {
    switch params.coloring {
        case COLORING_SOURCE: {
            let size = max(params.grid_max - params.grid_min, vec2(1e-6));
            let s = clamp((z - params.grid_min) / size, vec2(0.0), vec2(1.0));
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

    let seg = segment(instance_index);
    let a = to_screen(g(seg.start));
    let b = to_screen(g(seg.end));
    let df_start = f_prime(seg.start);
    let df_end = f_prime(seg.end);
    let z = select(seg.start, seg.end, at_end);
    let df = select(df_start, df_end, at_end);

    // Drop segments that are not finite, far off screen, or that jump across a
    // pole instead of following the curve. Across a pole the image runs off to
    // infinity and back in from the other side, so the chord points against
    // the curve's direction at the ends, and is often much longer than the
    // derivative predicts. Both ends are checked, never the midpoint: a
    // midpoint near the pole has a huge derivative that excuses any chord.
    // Every corner of the quad must reach the same verdict, so this uses only
    // per-segment values.
    let step = (seg.end - seg.start) * params.pixels_per_unit;
    let tangent_start = cmul(g_prime(df_start), step);
    let tangent_end = cmul(g_prime(df_end), step);
    let chord_vec = b - a;
    let chord = length(chord_vec);
    let expected = max(length(tangent_start), length(tangent_end));
    let forward = dot(chord_vec, tangent_start) >= 0.0 && dot(chord_vec, tangent_end) >= 0.0;
    // Tiny chords can point backwards at a critical point (f' = 0) without
    // jumping anywhere; keep those so no gap opens there.
    let follows_curve = (forward || chord < 4.0 * params.line_width)
        && chord < 4.0 * expected + params.line_width;
    let on_screen = all(abs(a) < vec2(MAX_SCREEN_COORD)) && all(abs(b) < vec2(MAX_SCREEN_COORD));
    if !(on_screen && follows_curve) {
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
    out.color = vec4(pow(line_color(z, seg.horizontal, df), vec3(2.2)), 1.0);
    return out;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    return in.color;
}
