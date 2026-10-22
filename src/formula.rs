// Formula parser and evaluator, following Rainmeter's Formulas page.
// A formula is parsed once; evaluating it is cheap, so Calc can run it every update.

const MAX_LEN: usize = 4096; // characters in one formula
const MAX_DEPTH: usize = 200; // nesting of parentheses, conditionals and unary signs

// What a [Name] reference asks for: its value, or its range information
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Suffix {
    Value,   // [Name] or just Name
    Percent, // [Name:%]
    Min,     // [Name:MinValue]
    Max,     // [Name:MaxValue]
}

// Where names in a formula get their values from
pub trait Env {
    // `name` is lowercase
    fn get(&self, name: &str, suffix: Suffix) -> Option<f64>;
}

// No names available (formulas inside ordinary options)
pub struct NoEnv;

impl Env for NoEnv {
    fn get(&self, _name: &str, _suffix: Suffix) -> Option<f64> {
        None
    }
}

#[derive(Clone, Copy)]
pub enum Un {
    Neg,
    Plus,
    Not, // bitwise ~
}

#[derive(Clone, Copy)]
pub enum Bin {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Rem,
    And,
    Or,
    Xor,
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    LAnd,
    LOr,
}

#[derive(Clone, Copy)]
pub enum Func {
    Cos,
    Sin,
    Tan,
    Acos,
    Asin,
    Atan,
    Atan2,
    Rad,
    Deg,
    Abs,
    Neg,
    Exp,
    Log,
    Ln,
    Sqrt,
    Sgn,
    Frac,
    Trunc,
    Floor,
    Ceil,
    Min,
    Max,
    Clamp,
    Round,
}

pub enum Expr {
    Num(f64),
    Var(String, Suffix),
    Unary(Un, Box<Expr>),
    Binary(Bin, Box<Expr>, Box<Expr>),
    Cond(Box<Expr>, Box<Expr>, Box<Expr>),
    Call(Func, Vec<Expr>),
}

// ---------- tokenizer ----------

#[derive(Clone)]
enum Tok {
    Num(f64),
    Ident(String),       // lowercase
    Ref(String, Suffix), // [Name] or [Name:%], lowercase
    Op(&'static str),
    End,
}

static END: Tok = Tok::End;

fn tokenize(text: &str) -> Result<Vec<Tok>, String> {
    if text.len() > MAX_LEN {
        return Err(format!("formula is longer than {} characters", MAX_LEN));
    }
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut toks = Vec::new();
    let mut i = 0;

    while i < len {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }

        if c.is_ascii_digit() {
            // Other bases: 0b / 0o / 0x (lowercase only, like Rainmeter)
            if c == '0' && i + 1 < len {
                let base = match chars[i + 1] {
                    'b' => Some(2),
                    'o' => Some(8),
                    'x' => Some(16),
                    _ => None,
                };
                if let Some(base) = base {
                    let start = i + 2;
                    let mut j = start;
                    while j < len && chars[j].is_digit(base) {
                        j += 1;
                    }
                    if j == start {
                        return Err(format!("'0{}' must be followed by digits", chars[i + 1]));
                    }
                    let digits: String = chars[start..j].iter().collect();
                    let n = u64::from_str_radix(&digits, base).map_err(|_| {
                        format!("number '0{}{}' is too large", chars[i + 1], digits)
                    })?;
                    toks.push(Tok::Num(n as f64));
                    i = j;
                    continue;
                }
            }
            // Plain decimal: digits, optionally followed by . and more digits
            let start = i;
            while i < len && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i + 1 < len && chars[i] == '.' && chars[i + 1].is_ascii_digit() {
                i += 1;
                while i < len && chars[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            let n = s.parse::<f64>().map_err(|_| format!("bad number '{}'", s))?;
            toks.push(Tok::Num(n));
            continue;
        }

        if c.is_ascii_alphabetic() || c == '_' {
            let start = i;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let name: String = chars[start..i].iter().collect();
            toks.push(Tok::Ident(name.to_lowercase()));
            continue;
        }

        if c == '[' {
            let end = chars[i + 1..]
                .iter()
                .position(|&ch| ch == ']')
                .ok_or_else(|| "missing ']'".to_string())?
                + i
                + 1;
            let inner: String = chars[i + 1..end].iter().collect();
            let (name, suffix) = match inner.rsplit_once(':') {
                Some((n, s)) => {
                    let suffix = match s.trim().to_lowercase().as_str() {
                        "%" => Suffix::Percent,
                        "minvalue" => Suffix::Min,
                        "maxvalue" => Suffix::Max,
                        other => return Err(format!("unsupported ':{}' in [{}]", other, inner)),
                    };
                    (n, suffix)
                }
                None => (inner.as_str(), Suffix::Value),
            };
            toks.push(Tok::Ref(name.trim().to_lowercase(), suffix));
            i = end + 1;
            continue;
        }

        // Two-character operators first
        let two: String = chars[i..(i + 2).min(len)].iter().collect();
        let op2 = match two.as_str() {
            "**" => Some("**"),
            "<>" => Some("<>"),
            "<=" => Some("<="),
            ">=" => Some(">="),
            "&&" => Some("&&"),
            "||" => Some("||"),
            _ => None,
        };
        if let Some(op) = op2 {
            toks.push(Tok::Op(op));
            i += 2;
            continue;
        }

        let op1 = match c {
            '+' => "+",
            '-' => "-",
            '*' => "*",
            '/' => "/",
            '%' => "%",
            '&' => "&",
            '|' => "|",
            '^' => "^",
            '~' => "~",
            '(' => "(",
            ')' => ")",
            '?' => "?",
            ':' => ":",
            ',' => ",",
            '=' => "=",
            '<' => "<",
            '>' => ">",
            '.' => return Err("numbers must start with a digit (write 0.5, not .5)".to_string()),
            other => return Err(format!("unexpected character '{}'", other)),
        };
        toks.push(Tok::Op(op1));
        i += 1;
    }
    Ok(toks)
}

fn describe(t: &Tok) -> String {
    match t {
        Tok::Num(n) => format!("number {}", n),
        Tok::Ident(s) => format!("name '{}'", s),
        Tok::Ref(s, _) => format!("reference [{}]", s),
        Tok::Op(o) => format!("'{}'", o),
        Tok::End => "end of the formula".to_string(),
    }
}

// ---------- parser ----------
// Precedence, lowest to highest: ?:  ||  &&  comparisons  |  ^  &  + -  * / %  unary  **
// (Rainmeter doesn't document its precedence; this is the usual Rust/Python order.)

const LEVELS: &[&[(&str, Bin)]] = &[
    &[("||", Bin::LOr)],
    &[("&&", Bin::LAnd)],
    &[
        ("=", Bin::Eq),
        ("<>", Bin::Ne),
        ("<=", Bin::Le),
        (">=", Bin::Ge),
        ("<", Bin::Lt),
        (">", Bin::Gt),
    ],
    &[("|", Bin::Or)],
    &[("^", Bin::Xor)],
    &[("&", Bin::And)],
    &[("+", Bin::Add), ("-", Bin::Sub)],
    &[("*", Bin::Mul), ("/", Bin::Div), ("%", Bin::Rem)],
];

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> &Tok {
        self.toks.get(self.pos).unwrap_or(&END)
    }

    fn next(&mut self) -> Tok {
        let t = self.peek().clone();
        self.pos += 1;
        t
    }

    fn eat_op(&mut self, op: &str) -> bool {
        if matches!(self.peek(), Tok::Op(o) if *o == op) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn expect_op(&mut self, op: &str) -> Result<(), String> {
        if self.eat_op(op) {
            Ok(())
        } else {
            Err(format!("expected '{}' but found {}", op, describe(self.peek())))
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            Err("formula is nested too deeply".to_string())
        } else {
            Ok(())
        }
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    // condition ? a : b
    fn parse_cond(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let cond = self.parse_binary(0)?;
        let result = if self.eat_op("?") {
            let yes = self.parse_cond()?;
            self.expect_op(":")?;
            let no = self.parse_cond()?;
            Expr::Cond(Box::new(cond), Box::new(yes), Box::new(no))
        } else {
            cond
        };
        self.leave();
        Ok(result)
    }

    fn parse_binary(&mut self, level: usize) -> Result<Expr, String> {
        if level == LEVELS.len() {
            return self.parse_unary();
        }
        let mut left = self.parse_binary(level + 1)?;
        loop {
            let found: Option<Bin> = LEVELS[level]
                .iter()
                .find(|(sym, _)| matches!(self.peek(), Tok::Op(o) if o == sym))
                .map(|&(_, b)| b);
            match found {
                Some(op) => {
                    self.pos += 1;
                    let right = self.parse_binary(level + 1)?;
                    left = Expr::Binary(op, Box::new(left), Box::new(right));
                }
                None => return Ok(left),
            }
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        self.enter()?;
        let op = if self.eat_op("-") {
            Some(Un::Neg)
        } else if self.eat_op("+") {
            Some(Un::Plus)
        } else if self.eat_op("~") {
            Some(Un::Not)
        } else {
            None
        };
        let result = match op {
            Some(op) => Expr::Unary(op, Box::new(self.parse_unary()?)),
            None => self.parse_power()?,
        };
        self.leave();
        Ok(result)
    }

    // ** is right-associative and binds tighter than a minus sign on its left
    fn parse_power(&mut self) -> Result<Expr, String> {
        let base = self.parse_primary()?;
        if self.eat_op("**") {
            let exponent = self.parse_unary()?;
            Ok(Expr::Binary(Bin::Pow, Box::new(base), Box::new(exponent)))
        } else {
            Ok(base)
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Ref(name, suffix) => Ok(Expr::Var(name, suffix)),
            Tok::Op("(") => {
                let inner = self.parse_cond()?;
                self.expect_op(")")?;
                Ok(inner)
            }
            Tok::Ident(name) => {
                if self.eat_op("(") {
                    let func = function_by_name(&name)
                        .ok_or_else(|| format!("unknown function '{}'", name))?;
                    let mut args = Vec::new();
                    if !self.eat_op(")") {
                        loop {
                            args.push(self.parse_cond()?);
                            if self.eat_op(",") {
                                continue;
                            }
                            self.expect_op(")")?;
                            break;
                        }
                    }
                    check_arity(func, &name, args.len())?;
                    Ok(Expr::Call(func, args))
                } else {
                    match name.as_str() {
                        "pi" => Ok(Expr::Num(std::f64::consts::PI)),
                        "e" => Ok(Expr::Num(std::f64::consts::E)),
                        _ => Ok(Expr::Var(name, Suffix::Value)),
                    }
                }
            }
            Tok::Op(o) => Err(format!("unexpected '{}'", o)),
            Tok::End => Err("the formula ended unexpectedly".to_string()),
        }
    }
}

fn function_by_name(name: &str) -> Option<Func> {
    Some(match name {
        "cos" => Func::Cos,
        "sin" => Func::Sin,
        "tan" => Func::Tan,
        "acos" => Func::Acos,
        "asin" => Func::Asin,
        "atan" => Func::Atan,
        "atan2" => Func::Atan2,
        "rad" => Func::Rad,
        "deg" => Func::Deg,
        "abs" => Func::Abs,
        "neg" => Func::Neg,
        "exp" => Func::Exp,
        "log" => Func::Log,
        "ln" => Func::Ln,
        "sqrt" => Func::Sqrt,
        "sgn" => Func::Sgn,
        "frac" => Func::Frac,
        "trunc" => Func::Trunc,
        "floor" => Func::Floor,
        "ceil" => Func::Ceil,
        "min" => Func::Min,
        "max" => Func::Max,
        "clamp" => Func::Clamp,
        "round" => Func::Round,
        _ => return None,
    })
}

fn check_arity(func: Func, name: &str, got: usize) -> Result<(), String> {
    let (min, max) = match func {
        Func::Atan2 | Func::Min | Func::Max => (2, 2),
        Func::Clamp => (3, 3),
        Func::Round => (1, 2),
        _ => (1, 1),
    };
    if got < min || got > max {
        return Err(if min == max {
            format!("{}() takes {} argument(s), but got {}", name, min, got)
        } else {
            format!("{}() takes {} to {} arguments, but got {}", name, min, max, got)
        });
    }
    Ok(())
}

// ---------- evaluator ----------

fn to_int(v: f64) -> i64 {
    v as i64 // truncates; saturates instead of overflowing
}

fn flag(b: bool) -> f64 {
    if b { 1.0 } else { 0.0 }
}

fn call(func: Func, a: &[f64]) -> f64 {
    let arg = |i: usize| a.get(i).copied().unwrap_or(0.0);
    let x = arg(0);
    match func {
        Func::Cos => x.cos(),
        Func::Sin => x.sin(),
        Func::Tan => x.tan(),
        Func::Acos => x.acos(),
        Func::Asin => x.asin(),
        Func::Atan => x.atan(),
        Func::Atan2 => x.atan2(arg(1)), // Atan2(y, x)
        Func::Rad => x.to_radians(),
        Func::Deg => x.to_degrees(),
        Func::Abs => x.abs(),
        Func::Neg => -x,
        Func::Exp => x.exp(),
        Func::Log => x.log10(),
        Func::Ln => x.ln(),
        Func::Sqrt => x.sqrt(),
        Func::Sgn => {
            if x > 0.0 {
                1.0
            } else if x < 0.0 {
                -1.0
            } else {
                0.0
            }
        }
        Func::Frac => x - x.trunc(),
        Func::Trunc => x.trunc(),
        Func::Floor => x.floor(),
        Func::Ceil => x.ceil(),
        Func::Min => x.min(arg(1)),
        Func::Max => x.max(arg(1)),
        // max then min, because f64::clamp panics when low > high
        Func::Clamp => x.max(arg(1)).min(arg(2)),
        Func::Round => {
            if a.len() > 1 {
                let places = arg(1).trunc().clamp(-15.0, 15.0) as i32;
                let scale = 10f64.powi(places);
                (x * scale).round() / scale
            } else {
                x.round()
            }
        }
    }
}

impl Expr {
    // Any problem, including a result that isn't a finite number, is an Err
    pub fn eval(&self, env: &dyn Env) -> Result<f64, String> {
        let v = self.eval_inner(env)?;
        if v.is_finite() {
            Ok(v)
        } else {
            Err("the result is not a number".to_string())
        }
    }

    fn eval_inner(&self, env: &dyn Env) -> Result<f64, String> {
        match self {
            Expr::Num(n) => Ok(*n),
            Expr::Var(name, suffix) => env
                .get(name, *suffix)
                .ok_or_else(|| format!("unknown name '{}'", name)),
            Expr::Unary(op, e) => {
                let v = e.eval_inner(env)?;
                Ok(match op {
                    Un::Neg => -v,
                    Un::Plus => v,
                    Un::Not => (!to_int(v)) as f64,
                })
            }
            // && and || only evaluate the right side when they need to
            Expr::Binary(Bin::LAnd, a, b) => {
                Ok(flag(a.eval_inner(env)? != 0.0 && b.eval_inner(env)? != 0.0))
            }
            Expr::Binary(Bin::LOr, a, b) => {
                Ok(flag(a.eval_inner(env)? != 0.0 || b.eval_inner(env)? != 0.0))
            }
            Expr::Binary(op, a, b) => {
                let x = a.eval_inner(env)?;
                let y = b.eval_inner(env)?;
                Ok(match op {
                    Bin::Add => x + y,
                    Bin::Sub => x - y,
                    Bin::Mul => x * y,
                    Bin::Div => {
                        if y == 0.0 {
                            return Err("division by zero".to_string());
                        }
                        x / y
                    }
                    Bin::Pow => x.powf(y),
                    Bin::Rem => {
                        if y == 0.0 {
                            return Err("remainder by zero".to_string());
                        }
                        x % y
                    }
                    Bin::And => (to_int(x) & to_int(y)) as f64,
                    Bin::Or => (to_int(x) | to_int(y)) as f64,
                    Bin::Xor => (to_int(x) ^ to_int(y)) as f64,
                    Bin::Eq => flag(x == y),
                    Bin::Ne => flag(x != y),
                    Bin::Lt => flag(x < y),
                    Bin::Gt => flag(x > y),
                    Bin::Le => flag(x <= y),
                    Bin::Ge => flag(x >= y),
                    Bin::LAnd | Bin::LOr => 0.0, // handled above
                })
            }
            Expr::Cond(c, yes, no) => {
                if c.eval_inner(env)? != 0.0 {
                    yes.eval_inner(env)
                } else {
                    no.eval_inner(env)
                }
            }
            Expr::Call(func, args) => {
                let mut values = Vec::with_capacity(args.len());
                for a in args {
                    values.push(a.eval_inner(env)?);
                }
                Ok(call(*func, &values))
            }
        }
    }
}

// ---------- public entry points ----------

pub fn parse(text: &str) -> Result<Expr, String> {
    let toks = tokenize(text)?;
    let mut parser = Parser { toks, pos: 0, depth: 0 };
    let expr = parser.parse_cond()?;
    if !matches!(parser.peek(), Tok::End) {
        return Err(format!("unexpected {} after the formula", describe(parser.peek())));
    }
    Ok(expr)
}

// Reads a numeric option: a plain number, or a (formula) in parentheses.
// The result must be a finite number.
pub fn parse_number(text: &str) -> Result<f64, String> {
    let t = text.trim();
    if t.starts_with('(') && t.ends_with(')') {
        return parse(t)?.eval(&NoEnv);
    }
    match t.parse::<f64>() {
        Ok(n) if n.is_finite() => Ok(n),
        _ => Err("not a number".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calc(text: &str) -> Result<f64, String> {
        parse(text)?.eval(&NoEnv)
    }

    struct TestEnv;

    impl Env for TestEnv {
        fn get(&self, name: &str, suffix: Suffix) -> Option<f64> {
            match (name, suffix) {
                ("a", Suffix::Value) => Some(4.0),
                ("a", Suffix::Percent) => Some(25.0),
                _ => None,
            }
        }
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(calc("2 + 3 * 4"), Ok(14.0));
        assert_eq!(calc("(2 + 3) * 4"), Ok(20.0));
        assert_eq!(calc("10 - 4 - 3"), Ok(3.0));
        assert_eq!(calc("2 ** 3 ** 2"), Ok(512.0));
        assert_eq!(calc("-2 ** 2"), Ok(-4.0));
        assert_eq!(calc("7 % 4"), Ok(3.0));
    }

    #[test]
    fn comparisons_logic_and_conditionals() {
        assert_eq!(calc("(1 < 2) && (3 = 3)"), Ok(1.0));
        assert_eq!(calc("5 <> 5"), Ok(0.0));
        assert_eq!(calc("5 < 6 ? 10 : 20"), Ok(10.0));
        assert_eq!(calc("2 < 1 ? 99 : (2 < 2 ? 98 : (2 < 3 ? 97 : 96))"), Ok(97.0));
    }

    #[test]
    fn functions_and_constants() {
        assert_eq!(calc("Max(2, 5)"), Ok(5.0));
        assert_eq!(calc("Clamp(15, 0, 10)"), Ok(10.0));
        assert_eq!(calc("Round(3.14159, 2)"), Ok(3.14));
        assert_eq!(calc("Round(2.5)"), Ok(3.0));
        assert_eq!(calc("Sqrt(16) + Abs(-3)"), Ok(7.0));
        assert!((calc("Sin(PI / 2)").unwrap() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn other_bases_and_bitwise() {
        assert_eq!(calc("0xF1"), Ok(241.0));
        assert_eq!(calc("0b110110"), Ok(54.0));
        assert_eq!(calc("0o123"), Ok(83.0));
        assert_eq!(calc("6 & 3"), Ok(2.0));
        assert_eq!(calc("6 | 3"), Ok(7.0));
        assert_eq!(calc("6 ^ 3"), Ok(5.0));
        assert_eq!(calc("~0"), Ok(-1.0));
    }

    #[test]
    fn names_come_from_the_environment() {
        assert_eq!(parse("a * 2 + [A]").unwrap().eval(&TestEnv), Ok(12.0));
        assert_eq!(parse("[a:%]").unwrap().eval(&TestEnv), Ok(25.0));
        assert!(parse("zzz + 1").unwrap().eval(&TestEnv).is_err());
    }

    #[test]
    fn mistakes_are_errors_not_crashes() {
        assert!(calc("1 / 0").is_err());
        assert!(calc(".5").is_err());
        assert!(calc("1 +").is_err());
        assert!(calc("Nope(1)").is_err());
        assert!(calc("Max(1)").is_err());
        assert!(calc("x + 1").is_err());
        assert!(calc("Sqrt(0 - 1)").is_err());
        assert!(calc("5 $ 5").is_err());
        let deep = format!("{}1{}", "(".repeat(500), ")".repeat(500));
        assert!(calc(&deep).is_err());
        assert!(calc(&"1+".repeat(5000)).is_err());
    }

    #[test]
    fn thirty_nested_conditionals_work() {
        let mut f = String::from("0");
        for i in 0..30 {
            f = format!("(1 = 1 ? {} : {})", i, f);
        }
        assert_eq!(calc(&f), Ok(29.0));
    }

    #[test]
    fn numeric_options() {
        assert_eq!(parse_number("(10 * 2)"), Ok(20.0));
        assert_eq!(parse_number(" 5 "), Ok(5.0));
        assert!(parse_number("NaN").is_err());
        assert!(parse_number("abc").is_err());
        assert!(parse_number("(1 / 0)").is_err());
    }
}