// Complex numbers as vec2(re, im). Add and subtract with + and -.
//
// Appended to src/grid.wgsl at runtime, together with the `f` generated from the
// formula in the control window (src/expr.rs), which calls these helpers.

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
// Principal branch: the cut runs along the negative real axis.
fn clog(z: vec2<f32>) -> vec2<f32> {
    return vec2(log(length(z)), atan2(z.y, z.x));
}
fn csqrt(z: vec2<f32>) -> vec2<f32> {
    let angle = 0.5 * atan2(z.y, z.x);
    return sqrt(length(z)) * vec2(cos(angle), sin(angle));
}
fn cpow(z: vec2<f32>, w: vec2<f32>) -> vec2<f32> {
    return cexp(cmul(w, clog(z)));
}
// z^n by repeated squaring: exact, and no branch cut.
fn cpowi(z: vec2<f32>, n: i32) -> vec2<f32> {
    var result = ONE;
    var base = z;
    var k = u32(abs(n));
    while k > 0u {
        if (k & 1u) == 1u {
            result = cmul(result, base);
        }
        base = cmul(base, base);
        k = k >> 1u;
    }
    if n < 0 {
        return cdiv(ONE, result);
    }
    return result;
}
fn csin(z: vec2<f32>) -> vec2<f32> {
    return vec2(sin(z.x) * cosh(z.y), cos(z.x) * sinh(z.y));
}
fn ccos(z: vec2<f32>) -> vec2<f32> {
    return vec2(cos(z.x) * cosh(z.y), -sin(z.x) * sinh(z.y));
}
fn ctan(z: vec2<f32>) -> vec2<f32> {
    return cdiv(csin(z), ccos(z));
}
fn csinh(z: vec2<f32>) -> vec2<f32> {
    return vec2(sinh(z.x) * cos(z.y), cosh(z.x) * sin(z.y));
}
fn ccosh(z: vec2<f32>) -> vec2<f32> {
    return vec2(cosh(z.x) * cos(z.y), sinh(z.x) * sin(z.y));
}
fn ctanh(z: vec2<f32>) -> vec2<f32> {
    return cdiv(csinh(z), ccosh(z));
}
