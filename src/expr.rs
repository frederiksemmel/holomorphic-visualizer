//! Formulas typed by the user, like `p * (1 - z) / (1 + z)`, turned into WGSL.
//!
//! The grid is mapped on the GPU, so a formula is never evaluated here: it is
//! parsed and emitted as a WGSL expression over the complex helpers in
//! `src/complex.wgsl`, which becomes the body of `f` in the grid shader.
//!
//! Syntax: numbers, `z`, `p`, `i`, `pi` (or `π`), `e`; `+ - * / ^` with the
//! usual precedence (`^` binds tightest and to the right); parentheses; and the
//! functions in [`FUNCTIONS`], called with parentheses. Juxtaposition
//! multiplies, so `2pi i z` and `p sin(z)` work, at the precedence of `*`.

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
const MAX_INTEGER_POWER: f32 = 64.0;
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
    Ok(expr.to_wgsl())
}

/// The WGSL definition of `f` with `body` as its value.
pub fn function_wgsl(body: &str) -> String {
    format!("fn f(z: vec2<f32>, p: vec2<f32>) -> vec2<f32> {{\n    return {body};\n}}\n")
}

#[derive(Clone, Debug, PartialEq)]
enum TokenKind {
    Number(f32),
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
                let value: f32 = text.parse().map_err(|_| ParseError {
                    message: format!("'{text}' is not a number"),
                    position: start,
                })?;
                if !value.is_finite() {
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
    Number(f32),
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

impl Expr {
    fn to_wgsl(&self) -> String {
        match self {
            // `{:?}` always writes a decimal point or exponent, so WGSL reads a float.
            Expr::Number(n) => format!("vec2({n:?}, 0.0)"),
            Expr::Z => "z".into(),
            Expr::P => "p".into(),
            Expr::I => "I".into(),
            Expr::Neg(a) => format!("(-{})", a.to_wgsl()),
            Expr::Add(a, b) => format!("({} + {})", a.to_wgsl(), b.to_wgsl()),
            Expr::Sub(a, b) => format!("({} - {})", a.to_wgsl(), b.to_wgsl()),
            Expr::Mul(a, b) => format!("cmul({}, {})", a.to_wgsl(), b.to_wgsl()),
            Expr::Div(a, b) => format!("cdiv({}, {})", a.to_wgsl(), b.to_wgsl()),
            Expr::Pow(a, b) => match b.integer() {
                Some(n) => format!("cpowi({}, {n})", a.to_wgsl()),
                None => format!("cpow({}, {})", a.to_wgsl(), b.to_wgsl()),
            },
            Expr::Call(function, a) => format!("{function}({})", a.to_wgsl()),
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
                "pi" => Ok(Expr::Number(std::f32::consts::PI)),
                "e" => Ok(Expr::Number(std::f32::consts::E)),
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
