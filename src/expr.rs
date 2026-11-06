//! Formulas typed by the user, like `p * (1 - z) / (1 + z)`, turned into WGSL.
//!
//! The grid is mapped on the GPU: a formula is parsed and emitted as a WGSL
//! expression over the complex helpers in `src/complex.wgsl`, which becomes
//! the body of `f` in the grid shader. [`Function::eval`] evaluates the same
//! parse tree on the CPU, for the few points the mouse needs (which grid
//! border is under the cursor); it mirrors those helpers, branch cuts included,
//! so it agrees with what is drawn.
//!
//! The parse tree also says how a formula is composed: [`Function::stages`]
//! takes it apart, and a [`Homotopy`] is the pair of functions on show.
//!
//! Syntax: numbers, `z`, `p`, `i`, `pi` (or `π`), `e`; `+ - * / ^` with the
//! usual precedence (`^` binds tightest and to the right); parentheses; and the
//! functions in [`FUNCTIONS`], called with parentheses. Juxtaposition
//! multiplies, so `2pi i z` and `p sin(z)` work, at the precedence of `*`.

use num::Complex;
use std::fmt;

/// Functions a formula can call, and the `src/complex.wgsl` helper for each.
pub const FUNCTIONS: [(&str, &str); 10] = [
    ("exp", "cexp"),
    ("log", "clog"),
    ("ln", "clog"),
    ("sqrt", "csqrt"),
    ("sin", "csin"),
    ("cos", "ccos"),
    ("tan", "ctan"),
    ("sinh", "csinh"),
    ("cosh", "ccosh"),
    ("tanh", "ctanh"),
];

/// Integer powers up to this size are multiplied out (no branch cut);
/// others go through `exp(w log z)`.
const MAX_INTEGER_POWER: f64 = 64.0;
/// Parentheses and operators nested deeper than this are rejected, so the
/// recursive parser can't overflow the stack.
const MAX_DEPTH: usize = 64;
/// Longer formulas are rejected, which also bounds how deep the expression
/// tree (and the recursion that turns it into WGSL) can get.
const MAX_TOKENS: usize = 1000;

/// Why a formula could not be read, and where (in characters from the start).
#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} (at character {})", self.message, self.position + 1)
    }
}

impl std::error::Error for ParseError {}

/// Translates `formula` into a WGSL expression in `z` and `p`, both `vec2<f32>`.
pub fn to_wgsl(formula: &str) -> Result<String, ParseError> {
    parse(formula).map(|function| function.to_wgsl())
}

/// Reads `formula` as a function of `z` and `p`.
pub fn parse(formula: &str) -> Result<Function, ParseError> {
    let tokens = tokenize(formula)?;
    if tokens.len() > MAX_TOKENS {
        return Err(ParseError {
            message: format!("too long: more than {MAX_TOKENS} symbols"),
            position: tokens[MAX_TOKENS].position,
        });
    }
    let mut parser = Parser {
        tokens,
        next: 0,
        depth: 0,
        end: formula.chars().count(),
    };
    let expr = parser.expression()?;
    if let Some(token) = parser.peek() {
        return Err(parser.error_at(token.position, format!("unexpected {}", token.kind)));
    }
    Ok(Function(expr))
}

/// A formula that parsed: f(z, p), as WGSL for the GPU or evaluated here.
#[derive(Clone, Debug, PartialEq)]
pub struct Function(Expr);

impl Function {
    pub fn is_identity(&self) -> bool {
        self.0 == Expr::Z
    }

    /// The stages with the identity before them, as stage 0: what a route
    /// through the function (see `crate::route`) picks from.
    pub fn chain(&self) -> Vec<Function> {
        let mut chain = vec![Function::identity()];
        chain.extend(self.stages());
        chain
    }

    /// The steps that build f, innermost first, up to f itself. The identity
    /// is not one of them: it is where the first step starts from.
    ///
    /// Walking down from the root, a node with exactly one operand built from
    /// z is a step, and the walk goes on into that operand. Whatever else it
    /// holds belongs to the step: a constant, p, or a bare z, which has nothing
    /// to take apart. So p * z * z, which is (p * z) * z, is p·z then p·z·z.
    /// A node with several such operands has no single step before it, so
    /// that is where the walk stops: sin(z) * cos(z) is one stage.
    pub fn stages(&self) -> Vec<Function> {
        let mut stages = vec![self.clone()];
        let mut node = &self.0;
        loop {
            let mut built = node
                .operands()
                .into_iter()
                .filter(|a| **a != Expr::Z && a.depends_on_z());
            match (built.next(), built.next()) {
                (Some(only), None) => node = only,
                _ => break,
            }
            stages.push(Function(node.clone()));
        }
        stages.reverse();
        stages
    }

    /// A WGSL expression in `z` and `p`, both `vec2<f32>`.
    pub fn to_wgsl(&self) -> String {
        self.0.to_wgsl()
    }

    /// f(z) = z.
    pub fn identity() -> Function {
        Function(Expr::Z)
    }

    /// f(z, p), computed the way the grid shader computes it.
    pub fn eval(&self, z: Complex<f64>, p: Complex<f64>) -> Complex<f64> {
        self.0.eval(z, p)
    }

    /// f'(z) by central differences, with the step `f_prime` in
    /// `src/grid.wgsl` uses: large enough that f64 rounding in f doesn't
    /// swamp the difference, and relative to |z| so it stays so far out.
    pub fn derivative(&self, z: Complex<f64>, p: Complex<f64>) -> Complex<f64> {
        if self.is_identity() {
            return Complex::new(1.0, 0.0);
        }
        let h = 1e-3 * z.norm().max(1.0);
        (self.eval(z + h, p) - self.eval(z - h, p)) / (2.0 * h)
    }

    /// The Taylor series of f around z. Exact, unlike [`Function::derivative`],
    /// which matters far zoomed in, where a pixel is a tiny step (see
    /// `crate::patch`).
    pub fn jet(&self, z: Complex<f64>, p: Complex<f64>) -> Jet {
        self.0.jet(z, p)
    }
}

/// The first terms of a Taylor series: a0 + a1·δ + a2·δ² + a3·δ³.
pub type Jet = [Complex<f64>; 4];

/// Multiplies series, dropping what is past the cubic.
fn jet_mul(a: Jet, b: Jet) -> Jet {
    std::array::from_fn(|k| (0..=k).map(|i| a[i] * b[k - i]).sum())
}

fn jet_div(a: Jet, b: Jet) -> Jet {
    let mut c = [Complex::new(0.0, 0.0); 4];
    for k in 0..4 {
        let known: Complex<f64> = (0..k).map(|i| c[i] * b[k - i]).sum();
        c[k] = cdiv(a[k] - known, b[0]);
    }
    c
}

/// The series of (g, h) where g' = u'·h and h' = sign·u'·g, as sin and cos, or
/// sinh and cosh, are, from their values at the centre.
fn jet_pair(u: Jet, g0: Complex<f64>, h0: Complex<f64>, sign: f64) -> (Jet, Jet) {
    let (mut g, mut h) = ([g0; 4], [h0; 4]);
    for k in 1..4 {
        let term = |x: &Jet| -> Complex<f64> {
            (1..=k).map(|i| u[i] * x[k - i] * i as f64).sum::<Complex<f64>>() / k as f64
        };
        (g[k], h[k]) = (term(&h), term(&g) * sign);
    }
    (g, h)
}

fn jet_exp(u: Jet) -> Jet {
    jet_pair(u, cexp(u[0]), cexp(u[0]), 1.0).0
}

fn jet_log(u: Jet) -> Jet {
    let mut l = [clog(u[0]); 4];
    for k in 1..4 {
        let known: Complex<f64> =
            (1..k).map(|i| l[i] * u[k - i] * i as f64).sum::<Complex<f64>>() / k as f64;
        l[k] = cdiv(u[k] - known, u[0]);
    }
    l
}

fn jet_sqrt(u: Jet) -> Jet {
    let mut s = [csqrt(u[0]); 4];
    for k in 1..4 {
        let known: Complex<f64> = (1..k).map(|i| s[i] * s[k - i]).sum();
        s[k] = cdiv(u[k] - known, 2.0 * s[0]);
    }
    s
}

/// As `cpowi`: repeated squaring, so no branch cut.
fn jet_powi(u: Jet, n: i32) -> Jet {
    let zero = Complex::new(0.0, 0.0);
    let one = [Complex::new(1.0, 0.0), zero, zero, zero];
    let (mut result, mut base, mut k) = (one, u, n.unsigned_abs());
    while k > 0 {
        if k & 1 == 1 {
            result = jet_mul(result, base);
        }
        base = jet_mul(base, base);
        k >>= 1;
    }
    if n < 0 { jet_div(one, result) } else { result }
}

/// Renders the formula back to text the parser reads as the same tree, with
/// `·` for `*` and only the parentheses it needs: `cos(p·z)`.
impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// The WGSL definition of `f` with `body` as its value.
pub fn function_wgsl(body: &str) -> String {
    format!("fn f(z: vec2<f32>, p: vec2<f32>) -> vec2<f32> {{\n    return {body};\n}}\n")
}

/// The two functions a leg of the homotopy runs between: `start` at t = 0,
/// `end` at t = 1. Everything on the CPU that draws or inverts the map works
/// from this pair, so that nothing takes the start for the identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Homotopy<'a> {
    pub start: &'a Function,
    pub end: &'a Function,
}

impl<'a> Homotopy<'a> {
    /// From stage `from` to stage `to` of a [`Function::chain`].
    pub fn between(chain: &'a [Function], from: usize, to: usize) -> Homotopy<'a> {
        Homotopy { start: &chain[from], end: &chain[to] }
    }

    /// At blend `t` the grid is where it started: every point is at z itself.
    pub fn unmapped(&self, t: f64) -> bool {
        self.start.is_identity() && (t == 0.0 || self.end.is_identity())
    }

    /// Where the grid point `z` is at blend `t`: h(z) = s(z) + t·(e(z) − s(z)),
    /// as `g` in `src/grid.wgsl` computes it, for the start s and end e. At
    /// t = 0 it is s(z) exactly, even where e blows up.
    pub fn map(&self, z: Complex<f64>, p: Complex<f64>, t: f64) -> Complex<f64> {
        let start = self.start.eval(z, p);
        if t == 0.0 {
            start
        } else {
            start + t * (self.end.eval(z, p) - start)
        }
    }

    /// h'(z) = s'(z) + t·(e'(z) − s'(z)).
    pub fn derivative(&self, z: Complex<f64>, p: Complex<f64>, t: f64) -> Complex<f64> {
        let start = self.start.derivative(z, p);
        if t == 0.0 {
            start
        } else {
            start + t * (self.end.derivative(z, p) - start)
        }
    }

    /// The Taylor series of [`Homotopy::map`] around `z`.
    pub fn jet(&self, z: Complex<f64>, p: Complex<f64>, t: f64) -> Jet {
        let (start, end) = (self.start.jet(z, p), self.end.jet(z, p));
        std::array::from_fn(|k| start[k] + t * (end[k] - start[k]))
    }
}

/// The WGSL definitions `src/grid.wgsl` needs: `stage(k, z, p)`, for every
/// stage of a [`Function::chain`]. They all go in one shader, so that moving
/// between them changes a uniform: recompiling a shader takes the grid off
/// screen for a moment. Each stage calls the one before it, which keeps the
/// source of a long chain linear in its length.
pub fn chain_wgsl(chain: &[Function]) -> String {
    let mut source = String::new();
    for k in 1..chain.len() {
        let inner = format!("stage{}(z, p)", k - 1);
        let inner = (k > 1).then_some((&chain[k - 1].0, inner.as_str()));
        let body = chain[k].0.wgsl_over(inner);
        source += &format!("fn stage{k}(z: vec2<f32>, p: vec2<f32>) -> vec2<f32> {{\n    return {body};\n}}\n");
    }
    source += "fn stage(k: u32, z: vec2<f32>, p: vec2<f32>) -> vec2<f32> {\n    switch k {\n";
    for k in 1..chain.len() {
        source += &format!("        case {k}u: {{ return stage{k}(z, p); }}\n");
    }
    source += "        default: { return z; }\n    }\n}\n";
    source
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Number(f64),
    Name(String),
    Operator(char),
    Open,
    Close,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenKind::Number(n) => write!(f, "number {n}"),
            TokenKind::Name(name) => write!(f, "'{name}'"),
            TokenKind::Operator(op) => write!(f, "'{op}'"),
            TokenKind::Open => write!(f, "'('"),
            TokenKind::Close => write!(f, "')'"),
        }
    }
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    position: usize,
}

fn tokenize(formula: &str) -> Result<Vec<Token>, ParseError> {
    let chars: Vec<char> = formula.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        let kind = match c {
            _ if c.is_whitespace() => {
                i += 1;
                continue;
            }
            '0'..='9' | '.' => {
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
                // An exponent, as in 1e-3, but not the constant e in 2e.
                let digit_at = |k: usize| chars.get(k).is_some_and(|c| c.is_ascii_digit());
                if matches!(chars.get(i), Some('e' | 'E')) {
                    let sign = matches!(chars.get(i + 1), Some('+' | '-'));
                    let digits = i + 1 + usize::from(sign);
                    if digit_at(digits) {
                        i = digits;
                        while digit_at(i) {
                            i += 1;
                        }
                    }
                }
                let text: String = chars[start..i].iter().collect();
                let value: f64 = text.parse().map_err(|_| ParseError {
                    message: format!("'{text}' is not a number"),
                    position: start,
                })?;
                // The formula also goes to the GPU, which has only f32.
                if !(value as f32).is_finite() {
                    return Err(ParseError {
                        message: format!("{text} is too large"),
                        position: start,
                    });
                }
                TokenKind::Number(value)
            }
            'π' => {
                i += 1;
                TokenKind::Name("pi".into())
            }
            _ if c.is_alphabetic() => {
                while i < chars.len() && chars[i].is_alphanumeric() {
                    i += 1;
                }
                TokenKind::Name(chars[start..i].iter().collect())
            }
            '+' | '-' | '*' | '/' | '^' | '·' | '×' | '−' | '÷' => {
                i += 1;
                TokenKind::Operator(match c {
                    '·' | '×' => '*',
                    '−' => '-',
                    '÷' => '/',
                    _ => c,
                })
            }
            '(' => {
                i += 1;
                TokenKind::Open
            }
            ')' => {
                i += 1;
                TokenKind::Close
            }
            _ => {
                return Err(ParseError {
                    message: format!("unexpected '{c}'"),
                    position: start,
                });
            }
        };
        tokens.push(Token {
            kind,
            position: start,
        });
    }
    Ok(tokens)
}

#[derive(Clone, Debug, PartialEq)]
enum Expr {
    Number(f64),
    Z,
    P,
    I,
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    /// A `src/complex.wgsl` helper applied to an argument.
    Call(&'static str, Box<Expr>),
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Parenthesized if it binds looser than the grammar allows here.
        let operand = |a: &Expr, at_least: u8| {
            if a.precedence() < at_least {
                format!("({a})")
            } else {
                a.to_string()
            }
        };
        match self {
            Expr::Number(n) if *n == std::f64::consts::PI => write!(f, "π"),
            Expr::Number(n) if *n == std::f64::consts::E => write!(f, "e"),
            Expr::Number(n) => write!(f, "{n}"),
            Expr::Z => write!(f, "z"),
            Expr::P => write!(f, "p"),
            Expr::I => write!(f, "i"),
            Expr::Neg(a) => write!(f, "-{}", operand(a, 3)),
            Expr::Add(a, b) => write!(f, "{} + {}", operand(a, 1), operand(b, 2)),
            Expr::Sub(a, b) => write!(f, "{} - {}", operand(a, 1), operand(b, 2)),
            Expr::Mul(a, b) => write!(f, "{}·{}", operand(a, 2), operand(b, 3)),
            Expr::Div(a, b) => write!(f, "{}/{}", operand(a, 2), operand(b, 3)),
            Expr::Pow(a, b) => write!(f, "{}^{}", operand(a, 5), operand(b, 3)),
            Expr::Call(helper, a) => {
                let name = FUNCTIONS.iter().find(|(_, h)| h == helper);
                write!(f, "{}({a})", name.map_or(*helper, |(name, _)| name))
            }
        }
    }
}

impl Expr {
    /// How tightly the outermost operator binds, by the rules of [`Parser`]:
    /// sums, then products, then signs, powers, and atoms.
    fn precedence(&self) -> u8 {
        match self {
            Expr::Add(..) | Expr::Sub(..) => 1,
            Expr::Mul(..) | Expr::Div(..) => 2,
            Expr::Neg(_) => 3,
            Expr::Pow(..) => 4,
            _ => 5,
        }
    }

    fn operands(&self) -> Vec<&Expr> {
        match self {
            Expr::Number(_) | Expr::Z | Expr::P | Expr::I => Vec::new(),
            Expr::Neg(a) | Expr::Call(_, a) => vec![a],
            Expr::Add(a, b)
            | Expr::Sub(a, b)
            | Expr::Mul(a, b)
            | Expr::Div(a, b)
            | Expr::Pow(a, b) => vec![a, b],
        }
    }

    fn depends_on_z(&self) -> bool {
        *self == Expr::Z || self.operands().iter().any(|a| a.depends_on_z())
    }

    fn to_wgsl(&self) -> String {
        self.wgsl_over(None)
    }

    /// As `to_wgsl`, with `inner.1`, a WGSL expression, for `inner.0`.
    fn wgsl_over(&self, inner: Option<(&Expr, &str)>) -> String {
        if let Some((known, call)) = inner
            && self == known
        {
            return call.to_owned();
        }
        let wgsl = |a: &Expr| a.wgsl_over(inner);
        match self {
            // `{:?}` always writes a decimal point or exponent, so WGSL reads a
            // float. The GPU has no f64, and rejects a literal that doesn't fit.
            Expr::Number(n) => format!("vec2({:?}, 0.0)", *n as f32),
            Expr::Z => "z".into(),
            Expr::P => "p".into(),
            Expr::I => "I".into(),
            Expr::Neg(a) => format!("(-{})", wgsl(a)),
            Expr::Add(a, b) => format!("({} + {})", wgsl(a), wgsl(b)),
            Expr::Sub(a, b) => format!("({} - {})", wgsl(a), wgsl(b)),
            Expr::Mul(a, b) => format!("cmul({}, {})", wgsl(a), wgsl(b)),
            Expr::Div(a, b) => format!("cdiv({}, {})", wgsl(a), wgsl(b)),
            Expr::Pow(a, b) => match b.integer() {
                Some(n) => format!("cpowi({}, {n})", wgsl(a)),
                None => format!("cpow({}, {})", wgsl(a), wgsl(b)),
            },
            Expr::Call(function, a) => format!("{function}({})", wgsl(a)),
        }
    }

    /// The Taylor series of `eval` around z, with the same branch cuts.
    fn jet(&self, z: Complex<f64>, p: Complex<f64>) -> Jet {
        let zero = Complex::new(0.0, 0.0);
        let constant = |a: Complex<f64>| [a, zero, zero, zero];
        let each = |a: Jet, b: Jet, join: fn(Complex<f64>, Complex<f64>) -> Complex<f64>| {
            std::array::from_fn(|k| join(a[k], b[k]))
        };
        match self {
            Expr::Number(n) => constant(Complex::new(*n, 0.0)),
            Expr::Z => [z, Complex::new(1.0, 0.0), zero, zero],
            Expr::P => constant(p),
            Expr::I => constant(Complex::i()),
            Expr::Neg(a) => a.jet(z, p).map(|x| -x),
            Expr::Add(a, b) => each(a.jet(z, p), b.jet(z, p), |x, y| x + y),
            Expr::Sub(a, b) => each(a.jet(z, p), b.jet(z, p), |x, y| x - y),
            Expr::Mul(a, b) => jet_mul(a.jet(z, p), b.jet(z, p)),
            Expr::Div(a, b) => jet_div(a.jet(z, p), b.jet(z, p)),
            Expr::Pow(a, b) => match b.integer() {
                Some(n) => jet_powi(a.jet(z, p), n),
                None => jet_exp(jet_mul(b.jet(z, p), jet_log(a.jet(z, p)))),
            },
            Expr::Call(function, a) => {
                let a = a.jet(z, p);
                let sin_cos = || jet_pair(a, csin(a[0]), ccos(a[0]), -1.0);
                let sinh_cosh = || jet_pair(a, csinh(a[0]), ccosh(a[0]), 1.0);
                match *function {
                    "cexp" => jet_exp(a),
                    "clog" => jet_log(a),
                    "csqrt" => jet_sqrt(a),
                    "csin" => sin_cos().0,
                    "ccos" => sin_cos().1,
                    "ctan" => {
                        let (sin, cos) = sin_cos();
                        jet_div(sin, cos)
                    }
                    "csinh" => sinh_cosh().0,
                    "ccosh" => sinh_cosh().1,
                    "ctanh" => {
                        let (sinh, cosh) = sinh_cosh();
                        jet_div(sinh, cosh)
                    }
                    _ => unreachable!("{function} is not in FUNCTIONS"),
                }
            }
        }
    }

    fn eval(&self, z: Complex<f64>, p: Complex<f64>) -> Complex<f64> {
        match self {
            Expr::Number(n) => Complex::new(*n, 0.0),
            Expr::Z => z,
            Expr::P => p,
            Expr::I => Complex::i(),
            Expr::Neg(a) => -a.eval(z, p),
            Expr::Add(a, b) => a.eval(z, p) + b.eval(z, p),
            Expr::Sub(a, b) => a.eval(z, p) - b.eval(z, p),
            Expr::Mul(a, b) => a.eval(z, p) * b.eval(z, p),
            Expr::Div(a, b) => cdiv(a.eval(z, p), b.eval(z, p)),
            Expr::Pow(a, b) => match b.integer() {
                Some(n) => cpowi(a.eval(z, p), n),
                None => cexp(b.eval(z, p) * clog(a.eval(z, p))),
            },
            Expr::Call(function, a) => {
                let a = a.eval(z, p);
                match *function {
                    "cexp" => cexp(a),
                    "clog" => clog(a),
                    "csqrt" => csqrt(a),
                    "csin" => csin(a),
                    "ccos" => ccos(a),
                    "ctan" => cdiv(csin(a), ccos(a)),
                    "csinh" => csinh(a),
                    "ccosh" => ccosh(a),
                    "ctanh" => cdiv(csinh(a), ccosh(a)),
                    _ => unreachable!("{function} is not in FUNCTIONS"),
                }
            }
        }
    }

    /// The value of a small integer literal like `3` or `-2`.
    fn integer(&self) -> Option<i32> {
        match self {
            Expr::Number(n) if n.fract() == 0.0 && n.abs() <= MAX_INTEGER_POWER => Some(*n as i32),
            Expr::Neg(a) => a.integer().map(|n| -n),
            _ => None,
        }
    }
}

// The helpers in `src/complex.wgsl`, in the same order of operations.
// Multiplication, addition and negation are num's, which compute the same.

fn cdiv(a: Complex<f64>, b: Complex<f64>) -> Complex<f64> {
    let d = b.re * b.re + b.im * b.im;
    Complex::new(
        (a.re * b.re + a.im * b.im) / d,
        (a.im * b.re - a.re * b.im) / d,
    )
}

fn cexp(z: Complex<f64>) -> Complex<f64> {
    z.re.exp() * Complex::new(z.im.cos(), z.im.sin())
}

/// Principal branch: the cut runs along the negative real axis.
fn clog(z: Complex<f64>) -> Complex<f64> {
    Complex::new(z.norm().ln(), z.im.atan2(z.re))
}

fn csqrt(z: Complex<f64>) -> Complex<f64> {
    let angle = 0.5 * z.im.atan2(z.re);
    z.norm().sqrt() * Complex::new(angle.cos(), angle.sin())
}

/// z^n by repeated squaring: exact, and no branch cut.
fn cpowi(z: Complex<f64>, n: i32) -> Complex<f64> {
    let mut result = Complex::new(1.0, 0.0);
    let mut base = z;
    let mut k = n.unsigned_abs();
    while k > 0 {
        if k & 1 == 1 {
            result *= base;
        }
        base = base * base;
        k >>= 1;
    }
    if n < 0 {
        cdiv(Complex::new(1.0, 0.0), result)
    } else {
        result
    }
}

fn csin(z: Complex<f64>) -> Complex<f64> {
    Complex::new(z.re.sin() * z.im.cosh(), z.re.cos() * z.im.sinh())
}

fn ccos(z: Complex<f64>) -> Complex<f64> {
    Complex::new(z.re.cos() * z.im.cosh(), -z.re.sin() * z.im.sinh())
}

fn csinh(z: Complex<f64>) -> Complex<f64> {
    Complex::new(z.re.sinh() * z.im.cos(), z.re.cosh() * z.im.sin())
}

fn ccosh(z: Complex<f64>) -> Complex<f64> {
    Complex::new(z.re.cosh() * z.im.cos(), z.re.sinh() * z.im.sin())
}

struct Parser {
    tokens: Vec<Token>,
    next: usize,
    depth: usize,
    /// Length of the formula, where errors about a missing ending point.
    end: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.next)
    }

    fn peek_operator(&self, ops: &[char]) -> Option<char> {
        match self.peek()?.kind {
            TokenKind::Operator(op) if ops.contains(&op) => Some(op),
            _ => None,
        }
    }

    fn position(&self) -> usize {
        self.peek().map_or(self.end, |token| token.position)
    }

    fn error_at(&self, position: usize, message: String) -> ParseError {
        ParseError { message, position }
    }

    /// Runs `parse` one level deeper, failing if formulas nest too deep.
    fn nested(
        &mut self,
        parse: impl FnOnce(&mut Self) -> Result<Expr, ParseError>,
    ) -> Result<Expr, ParseError> {
        if self.depth >= MAX_DEPTH {
            return Err(self.error_at(self.position(), "nested too deeply".into()));
        }
        self.depth += 1;
        let result = parse(self);
        self.depth -= 1;
        result
    }

    /// expression = term (('+' | '-') term)*
    fn expression(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.term()?;
        while let Some(op) = self.peek_operator(&['+', '-']) {
            self.next += 1;
            let right = self.term()?;
            left = match op {
                '+' => Expr::Add(left.into(), right.into()),
                _ => Expr::Sub(left.into(), right.into()),
            };
        }
        Ok(left)
    }

    /// term = unary (('*' | '/') unary | power)*, where a bare power is
    /// multiplied (`2pi`, `p sin(z)`).
    fn term(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.unary()?;
        loop {
            if let Some(op) = self.peek_operator(&['*', '/']) {
                self.next += 1;
                let right = self.unary()?;
                left = match op {
                    '*' => Expr::Mul(left.into(), right.into()),
                    _ => Expr::Div(left.into(), right.into()),
                };
            } else if matches!(
                self.peek().map(|t| &t.kind),
                Some(TokenKind::Number(_) | TokenKind::Name(_) | TokenKind::Open)
            ) {
                let right = self.power()?;
                left = Expr::Mul(left.into(), right.into());
            } else {
                return Ok(left);
            }
        }
    }

    /// unary = ('-' | '+') unary | power
    fn unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek_operator(&['-', '+']) {
            Some(op) => {
                self.next += 1;
                let operand = self.nested(Self::unary)?;
                Ok(if op == '-' {
                    Expr::Neg(operand.into())
                } else {
                    operand
                })
            }
            None => self.power(),
        }
    }

    /// power = atom ('^' unary)?, so `z^-1` works and `z^2^3` is `z^(2^3)`.
    fn power(&mut self) -> Result<Expr, ParseError> {
        let base = self.atom()?;
        if self.peek_operator(&['^']).is_some() {
            self.next += 1;
            let exponent = self.nested(Self::unary)?;
            return Ok(Expr::Pow(base.into(), exponent.into()));
        }
        Ok(base)
    }

    /// atom = number | name | function '(' expression ')' | '(' expression ')'
    fn atom(&mut self) -> Result<Expr, ParseError> {
        let Some(token) = self.peek().cloned() else {
            return Err(self.error_at(self.end, "formula ends too early".into()));
        };
        self.next += 1;
        match token.kind {
            TokenKind::Number(n) => Ok(Expr::Number(n)),
            TokenKind::Open => self.nested(|parser| parser.parenthesized(token.position)),
            TokenKind::Name(name) => match name.as_str() {
                "z" => Ok(Expr::Z),
                "p" => Ok(Expr::P),
                "i" => Ok(Expr::I),
                "pi" => Ok(Expr::Number(std::f64::consts::PI)),
                "e" => Ok(Expr::Number(std::f64::consts::E)),
                _ => {
                    let open = self.position();
                    let Some(&(_, helper)) = FUNCTIONS.iter().find(|(f, _)| *f == name) else {
                        let called = self.peek().map(|t| &t.kind) == Some(&TokenKind::Open);
                        return Err(self.error_at(token.position, unknown_name(&name, called)));
                    };
                    if self.peek().map(|t| &t.kind) != Some(&TokenKind::Open) {
                        return Err(
                            self.error_at(open, format!("write {name}(…) with parentheses"))
                        );
                    }
                    self.next += 1;
                    let argument = self.nested(|parser| parser.parenthesized(open))?;
                    Ok(Expr::Call(helper, argument.into()))
                }
            },
            kind => Err(self.error_at(token.position, format!("unexpected {kind}"))),
        }
    }

    /// The rest of a parenthesized expression, after the '(' at `open`.
    fn parenthesized(&mut self, open: usize) -> Result<Expr, ParseError> {
        let inner = self.expression()?;
        match self.peek() {
            Some(Token {
                kind: TokenKind::Close,
                ..
            }) => {
                self.next += 1;
                Ok(inner)
            }
            _ => Err(self.error_at(open, "this '(' is never closed".into())),
        }
    }
}

/// The error for a name that is neither a variable, a constant nor in
/// [`FUNCTIONS`]; `called` if a '(' follows it.
fn unknown_name(name: &str, called: bool) -> String {
    // Functions people try that are left out on purpose.
    if ["conj", "abs", "re", "im", "arg"].contains(&name.to_lowercase().as_str()) {
        return format!("'{name}' is left out: it isn't holomorphic, so its map isn't conformal");
    }
    if called {
        let names: Vec<&str> = FUNCTIONS.iter().map(|&(f, _)| f).collect();
        let (last, rest) = names.split_last().unwrap();
        return format!(
            "unknown function '{name}': use {} or {last}",
            rest.join(", ")
        );
    }
    format!("unknown name '{name}': use z, p, i, pi, e or a function")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::examples::EXAMPLES;

    /// Checks that `wgsl` (the body of `f`) compiles, together with the helpers.
    fn validate(body: &str) {
        let source = format!("{}\n{}", crate::COMPLEX_WGSL, function_wgsl(body));
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}\n{source}", e.emit_to_string(&source)));
        naga::valid::Validator::new(Default::default(), Default::default())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{e:?}\n{source}"));
    }

    #[test]
    fn generated_code_compiles() {
        for formula in [
            "z",
            "p * (1 - z) / (1 + z)",
            "exp(p z)",
            "z^p",
            "z^3 - 2z^-2 + z^0.5",
            "p sin(z) + cos(z) - tan(z)",
            "sinh(z) cosh(z) / tanh(z) + sqrt(z) + log(z) + ln(z)",
            "z + p / z",
            "p exp(2pi i z / (2pi i + log(256)))",
            "-z^2 + +z - -1e-3 + 1.5E+2 + π · e × z ÷ 2 − i",
        ] {
            let body = to_wgsl(formula).unwrap_or_else(|e| panic!("{formula}: {e}"));
            validate(&body);
        }
    }

    /// The stages of `formula`, as text.
    fn stages(formula: &str) -> Vec<String> {
        let stages = parse(formula).unwrap().stages();
        stages.iter().map(Function::to_string).collect()
    }

    #[test]
    fn stages_of_the_examples() {
        let expected: [(&str, &[&str]); 6] = [
            ("exp", &["p·z", "exp(p·z)"]),
            // Both sides of the division are built from z.
            ("mobius", &["p·(1 - z)/(1 + z)"]),
            ("power", &["z^p"]),
            ("sine", &["sin(z)", "p·sin(z)"]),
            ("joukowski", &["p/z", "z + p/z"]),
            (
                "escher",
                &[
                    "2·π·i·z",
                    "2·π·i·z/(2·π·i + log(p·e^6))",
                    "exp(2·π·i·z/(2·π·i + log(p·e^6)))",
                ],
            ),
        ];
        assert_eq!(expected.len(), EXAMPLES.len());
        for (slug, expected) in expected {
            let example = EXAMPLES.iter().find(|e| e.slug == slug).unwrap();
            assert_eq!(stages(example.formula), expected, "{slug}");
        }
    }

    #[test]
    fn a_composition_comes_apart_layer_by_layer() {
        assert_eq!(
            stages("sin(cos(p*z))"),
            ["p·z", "cos(p·z)", "sin(cos(p·z))"]
        );
        assert_eq!(stages("-exp(z)^2"), ["exp(z)", "exp(z)^2", "-exp(z)^2"]);
        assert_eq!(stages("2^z"), ["2^z"]);
        // By the shape of the tree: `2 * z * p` is (2 * z) * p.
        assert_eq!(stages("2 * z * p"), ["2·z", "2·z·p"]);
        assert_eq!(stages("2 * p * z"), ["2·p·z"]);
    }

    #[test]
    fn a_bare_z_beside_a_step_does_not_stop_the_walk() {
        assert_eq!(stages("p*z*z"), ["p·z", "p·z·z"]);
        assert_eq!(stages("p*z*z*z"), ["p·z", "p·z·z", "p·z·z·z"]);
        assert_eq!(stages("z*(p*z)"), ["p·z", "z·(p·z)"]);
        assert_eq!(stages("exp(z + 1/z)"), ["1/z", "z + 1/z", "exp(z + 1/z)"]);
    }

    #[test]
    fn several_operands_built_from_z_make_one_stage() {
        assert_eq!(stages("z * z"), ["z·z"]);
        assert_eq!(stages("z^z"), ["z^z"]);
        assert_eq!(stages("sin(z)cos(z)"), ["sin(z)·cos(z)"]);
        // Only the walk from the root counts: what is inside stays whole.
        assert_eq!(
            stages("log(1 + z^2 + exp(z))"),
            ["1 + z^2 + exp(z)", "log(1 + z^2 + exp(z))"]
        );
    }

    #[test]
    fn stages_leave_out_the_identity_and_end_at_the_formula() {
        let formulas = EXAMPLES.iter().map(|example| example.formula);
        let typed = ["z", "p", "2", "i", "sin(cos(p*z))", "-z", "z + 1", "1/z", "exp(exp(z))"];
        for formula in formulas.chain(typed) {
            let function = parse(formula).unwrap();
            let stages = function.stages();
            assert_eq!(stages[0].is_identity(), function.is_identity(), "{formula}");
            assert_eq!(stages.last(), Some(&function), "{formula}");
            // No stage twice.
            for pair in stages.windows(2) {
                assert_ne!(pair[0], pair[1], "{formula}");
            }
        }
        // Without z the formula is a single stage, and so is z alone.
        assert_eq!(stages("p + 1"), ["p + 1"]);
        assert_eq!(stages("z"), ["z"]);
    }

    #[test]
    fn long_chains_are_many_stages_and_stay_in_order() {
        assert_eq!(stages("z + 1 + 2 + 3").len(), 3);
        let nested = format!("{}z{}", "sin(".repeat(11), ")".repeat(11));
        let all = stages(&nested);
        assert_eq!(all.len(), 11);
        assert_eq!(all[0], "sin(z)");
        assert_eq!(all[10], nested);
    }

    #[test]
    fn text_reads_back_as_the_same_function() {
        let formulas = [
            "1 - (2 - z)",
            "1 - 2 - z",
            "1 / (2 / z)",
            "1 / 2 / z",
            "1 / (2 * z)",
            "(1 + z) * (2 - z)",
            "(z^2)^3",
            "z^2^3",
            "z^(1/2)",
            "z^-2",
            "z^-(2 + p)",
            "(-z)^2",
            "-z^2",
            "-(z + 1)",
            "-(z * 2)",
            "- -z",
            "2 * -z",
            "z - -1",
            "(-2) * z",
            "e^(i pi z)",
            "2e3 z + 1e-3 + 0.1",
            "ln(z) + sqrt(z) + tan(z) - tanh(z)",
            "(p + 1)(z - 1)",
            "1.5 π",
        ];
        for formula in formulas {
            let function = parse(formula).unwrap();
            let text = function.to_string();
            assert_eq!(parse(&text), Ok(function), "{formula} became {text}");
        }
        for example in &EXAMPLES {
            for stage in parse(example.formula).unwrap().stages() {
                assert_eq!(parse(&stage.to_string()), Ok(stage));
            }
        }
    }

    #[test]
    fn text_has_only_the_parentheses_it_needs() {
        let text = |formula: &str| parse(formula).unwrap().to_string();
        assert_eq!(text("((z)) + (1)"), "z + 1");
        assert_eq!(text("(p * z) + (1 / z)"), "p·z + 1/z");
        assert_eq!(text("1 - (2 + z)"), "1 - (2 + z)");
        assert_eq!(text("p (1 + z)"), "p·(1 + z)");
        assert_eq!(text("ln(z)"), "log(z)");
        assert_eq!(text("2pi"), "2·π");
    }

    fn homotopy(formula: &str, from: usize, to: usize) -> Homotopy<'static> {
        let chain = parse(formula).unwrap().chain();
        Homotopy::between(Box::leak(chain.into_boxed_slice()), from, to)
    }

    #[test]
    fn the_chain_is_the_identity_then_the_stages() {
        let chain: Vec<String> = parse("sin(cos(p*z))").unwrap().chain().iter().map(Function::to_string).collect();
        assert_eq!(chain, ["z", "p·z", "cos(p·z)", "sin(cos(p·z))"]);
        let start_and_end = |from, to| {
            let Homotopy { start, end } = homotopy("sin(cos(p*z))", from, to);
            (start.to_string(), end.to_string())
        };
        assert_eq!(start_and_end(1, 3), ("p·z".into(), "sin(cos(p·z))".into()));
    }

    #[test]
    fn the_map_runs_between_the_two_ends() {
        let p = Complex::new(0.5, -2.0);
        let z = Complex::new(0.3, 0.7);
        let ends = homotopy("sin(cos(p*z))", 1, 2);
        let (start, end) = (p * z, (p * z).cos());
        assert_close(ends.map(z, p, 0.0), start, "t = 0");
        assert_close(ends.map(z, p, 1.0), end, "t = 1");
        assert_close(ends.map(z, p, 0.25), start + 0.25 * (end - start), "t = 1/4");

        let (start, end) = (p, -p * (p * z).sin());
        let close = |a: Complex<f64>, b: Complex<f64>| (a - b).norm() <= 1e-3 * b.norm().max(1.0);
        assert!(close(ends.derivative(z, p, 0.0), start));
        assert!(close(ends.derivative(z, p, 1.0), end));
        assert!(close(ends.derivative(z, p, 0.25), start + 0.25 * (end - start)));
    }

    #[test]
    fn the_whole_function_blends_as_it_always_did() {
        let p = Complex::new(0.5, -2.0);
        let z = Complex::new(0.3, 0.7);
        let f = parse("exp(p * z)").unwrap();
        let ends = homotopy("exp(p * z)", 0, 2);
        let one = Complex::new(1.0, 0.0);
        for t in [0.25, 0.5, 1.0] {
            assert_eq!(ends.map(z, p, t), z + t * (f.eval(z, p) - z));
            assert_eq!(
                ends.derivative(z, p, t),
                one + t * (f.derivative(z, p) - one)
            );
        }
        // At the identity it is z exactly, even where f blows up.
        let pole = Complex::new(0.0, 0.0);
        assert_eq!(homotopy("1/z", 0, 1).map(pole, p, 0.0), pole);
        assert_eq!(homotopy("1/z", 0, 1).derivative(pole, p, 0.0), one);
    }

    #[test]
    fn only_the_identity_leaves_the_grid_unmapped() {
        let unmapped = |from, to, t| homotopy("sin(cos(p*z))", from, to).unmapped(t);
        assert!(unmapped(0, 3, 0.0) && !unmapped(0, 3, 0.5) && !unmapped(0, 3, 1.0));
        assert!(unmapped(0, 1, 0.0) && !unmapped(0, 1, 0.5));
        // Past the identity even t = 0 is mapped.
        assert!(!unmapped(1, 2, 0.0) && !unmapped(2, 3, 0.0));
        // The identity itself stays put all the way.
        assert!(homotopy("z", 0, 1).unmapped(1.0));
    }

    #[test]
    fn the_identity_has_an_exact_derivative() {
        let z = Complex::new(1234.5, -987.6);
        assert_eq!(parse("z").unwrap().derivative(z, z), Complex::new(1.0, 0.0));
    }

    #[test]
    fn the_shader_gets_every_stage_each_built_on_the_one_before() {
        let wgsl = chain_wgsl(&parse("sin(cos(p*z))").unwrap().chain());
        assert!(wgsl.contains("return cmul(p, z);"), "{wgsl}");
        assert!(wgsl.contains("return ccos(stage1(z, p));"), "{wgsl}");
        assert!(wgsl.contains("return csin(stage2(z, p));"), "{wgsl}");
        assert!(wgsl.contains("case 3u: { return stage3(z, p); }"), "{wgsl}");
        assert!(wgsl.contains("default: { return z; }"), "{wgsl}");
    }

    #[test]
    fn a_long_chain_makes_a_short_shader() {
        let formula = format!("z{}", " + 1".repeat(400));
        let chain = parse(&formula).unwrap().chain();
        assert_eq!(chain.len(), 401);
        assert!(chain_wgsl(&chain).len() < 100_000);
    }

    #[test]
    fn a_stage_keeps_what_its_step_adds() {
        let wgsl = chain_wgsl(&parse("p*z*z").unwrap().chain());
        assert!(wgsl.contains("return cmul(stage1(z, p), z);"), "{wgsl}");
    }

    #[test]
    fn precedence() {
        assert_eq!(
            to_wgsl("1 + 2 * z").unwrap(),
            "(vec2(1.0, 0.0) + cmul(vec2(2.0, 0.0), z))"
        );
        assert_eq!(to_wgsl("-z^2").unwrap(), "(-cpowi(z, 2))");
        assert_eq!(to_wgsl("z^-1").unwrap(), "cpowi(z, -1)");
        assert_eq!(to_wgsl("z^p^2").unwrap(), "cpow(z, cpowi(p, 2))");
        assert_eq!(to_wgsl("1 - z - p").unwrap(), "((vec2(1.0, 0.0) - z) - p)");
    }

    #[test]
    fn juxtaposition_multiplies() {
        assert_eq!(to_wgsl("2z").unwrap(), to_wgsl("2 * z").unwrap());
        assert_eq!(to_wgsl("p sin(z)").unwrap(), to_wgsl("p * sin(z)").unwrap());
        assert_eq!(
            to_wgsl("2(z + 1)").unwrap(),
            to_wgsl("2 * (z + 1)").unwrap()
        );
        // At the precedence of *, binding looser than ^.
        assert_eq!(to_wgsl("1 / 2z").unwrap(), to_wgsl("1 / 2 * z").unwrap());
        assert_eq!(to_wgsl("2z^2").unwrap(), to_wgsl("2 * z^2").unwrap());
        // `2e` is 2 times e, `2e3` is 2000.
        assert_eq!(to_wgsl("2e").unwrap(), to_wgsl("2 * e").unwrap());
        assert_eq!(to_wgsl("2e3").unwrap(), "vec2(2000.0, 0.0)");
    }

    #[test]
    fn errors_point_at_the_problem() {
        let error = |formula: &str| to_wgsl(formula).unwrap_err();
        assert_eq!(error("").position, 0);
        assert_eq!(error("z +").position, 3);
        assert_eq!(error("2 * w").position, 4);
        assert!(error("2 * w").message.contains("unknown name 'w'"));
        assert_eq!(error("1 + zeta(z)").position, 4);
        assert!(
            error("zeta(z)")
                .message
                .contains("unknown function 'zeta': use exp, log,")
        );
        assert!(error("zeta(z)").message.ends_with("cosh or tanh"));
        assert!(error("conj(z)").message.contains("isn't holomorphic"));
        assert!(error("Re(z)").message.contains("'Re' is left out"));
        assert_eq!(error("sin z").position, 4);
        assert_eq!(error("(z + 1").position, 0);
        assert_eq!(error("z)").position, 1);
        assert_eq!(error("z $ 2").position, 2);
        assert_eq!(error("1..2").position, 0);
        assert!(error("1e39").message.contains("too large"));
    }

    fn eval(formula: &str, z: Complex<f64>) -> Complex<f64> {
        parse(formula).unwrap().eval(z, Complex::new(0.5, -2.0))
    }

    fn assert_close(a: Complex<f64>, b: Complex<f64>, what: &str) {
        assert!((a - b).norm() <= 1e-5 * b.norm().max(1.0), "{what}: {a} vs {b}");
    }

    #[test]
    fn evaluates_known_values() {
        let i = Complex::i();
        let one = Complex::new(1.0, 0.0);
        // Integer powers are multiplied out, so these are exact.
        assert_eq!(eval("z^2", one + i), 2.0 * i);
        assert_eq!(eval("z^-1", i), -i);
        assert_eq!(eval("1/z", i), -i);
        assert_eq!(eval("p", i), Complex::new(0.5, -2.0));
        assert_eq!(eval("2pi i z", one), Complex::new(0.0, 2.0 * std::f64::consts::PI));
        assert_close(eval("exp(i pi)", one), -one, "exp(iπ)");
        // On the cut, the principal branch takes the upper side, as atan2 does.
        let minus_four = Complex::new(-4.0, 0.0);
        assert_close(eval("log(z)", Complex::new(-1.0, 0.0)), std::f64::consts::PI * i, "log(-1)");
        assert_close(eval("sqrt(z)", minus_four), 2.0 * i, "sqrt(-4)");
        assert_close(eval("z^0.5", minus_four), 2.0 * i, "(-4)^0.5");
        assert_close(eval("z^(1/2)", minus_four), 2.0 * i, "(-4)^(1/2)");
        // Below the cut, the other side.
        let below = Complex::new(-4.0, -1e-6);
        assert_close(eval("sqrt(z)", below), -2.0 * i, "sqrt just below -4");
    }

    #[test]
    fn evaluation_agrees_with_num() {
        let p = Complex::new(0.5, -2.0);
        let points = [
            Complex::new(0.3, 0.7),
            Complex::new(-1.2, 0.4),
            Complex::new(2.0, -1.5),
            Complex::new(-0.4, -0.9),
        ];
        let cases: [(&str, fn(Complex<f64>, Complex<f64>) -> Complex<f64>); 15] = [
            ("exp(z)", |z, _| z.exp()),
            ("log(z)", |z, _| z.ln()),
            ("ln(z)", |z, _| z.ln()),
            ("sqrt(z)", |z, _| z.sqrt()),
            ("sin(z)", |z, _| z.sin()),
            ("cos(z)", |z, _| z.cos()),
            ("tan(z)", |z, _| z.tan()),
            ("sinh(z)", |z, _| z.sinh()),
            ("cosh(z)", |z, _| z.cosh()),
            ("tanh(z)", |z, _| z.tanh()),
            ("z^p", |z, p| z.powc(p)),
            ("z^3", |z, _| z.powi(3)),
            ("z^-2", |z, _| z.powi(-2)),
            ("z^1.5", |z, _| z.powf(1.5)),
            ("p (1 - z) / (1 + z)", |z, p| p * (1.0 - z) / (1.0 + z)),
        ];
        for (formula, expected) in cases {
            let function = parse(formula).unwrap();
            for z in points {
                assert_close(function.eval(z, p), expected(z, p), &format!("{formula} at {z}"));
            }
        }
    }

    /// Each term of the series against a central difference of the one
    /// before, for every function and away from its branch cuts.
    #[test]
    fn jets_are_the_taylor_series() {
        let (z, p) = (Complex::new(0.7, 0.4), Complex::new(0.5, 0.25));
        let formulas = [
            "z", "p", "2", "-z", "z + p", "z - 1/z", "p*z*z", "z/(1 + z)", "z^2", "z^5", "z^-3",
            "z^p", "2^z", "z^z", "exp(z)", "log(z)", "ln(p*z)", "sqrt(z)", "sin(z)", "cos(z)",
            "tan(z)", "sinh(z)", "cosh(z)", "tanh(z)", "sin(cos(p*z))", "exp(z + 1/z)",
        ];
        let h = 1e-3;
        for formula in formulas {
            let f = parse(formula).unwrap();
            let jet = f.jet(z, p);
            assert_close(jet[0], f.eval(z, p), formula);
            for k in 1..4 {
                let at = |z| f.jet(z, p)[k - 1];
                let slope = (at(z + h) - at(z - h)) / (2.0 * h) / k as f64;
                assert!((jet[k] - slope).norm() < 1e-4 * slope.norm().max(1.0), "{formula}, a{k}");
            }
        }
    }

    #[test]
    fn a_jet_keeps_the_branch_of_eval() {
        // Below the cut, log(z) has argument −π, and sqrt(z) is near −i.
        let z = Complex::new(-1.0, -1e-6);
        let p = Complex::new(1.0, 0.0);
        for formula in ["log(z)", "sqrt(z)", "z^p"] {
            let f = parse(formula).unwrap();
            assert_close(f.jet(z, p)[0], f.eval(z, p), formula);
        }
    }

    #[test]
    fn derivative_by_finite_differences() {
        let p = Complex::new(1.0, 0.0);
        let close = |a: Complex<f64>, b: Complex<f64>| (a - b).norm() <= 1e-3 * b.norm().max(1.0);
        for z in [Complex::new(1.0, 1.0), Complex::new(-0.2, 0.3), Complex::new(8.0, -3.0)] {
            let cube = parse("z^3").unwrap().derivative(z, p);
            assert!(close(cube, 3.0 * z * z), "(z^3)' at {z}: {cube}");
            let exp = parse("exp(z)").unwrap().derivative(z, p);
            assert!(close(exp, z.exp()), "exp' at {z}: {exp}");
        }
    }

    #[test]
    fn deep_nesting_is_an_error_not_a_crash() {
        let nested =
            |message: Result<String, ParseError>| message.unwrap_err().message.contains("nested");
        assert!(nested(to_wgsl(&format!(
            "{}z{}",
            "(".repeat(100),
            ")".repeat(100)
        ))));
        assert!(nested(to_wgsl(&format!("{}z", "-".repeat(100)))));
        assert!(nested(to_wgsl(&format!("{}z", "z^".repeat(100)))));
        assert!(nested(to_wgsl(&format!(
            "{}z{}",
            "exp(".repeat(100),
            ")".repeat(100)
        ))));
        assert!(
            to_wgsl(&vec!["z"; 10_000].join("+"))
                .unwrap_err()
                .message
                .contains("too long")
        );
    }
}
