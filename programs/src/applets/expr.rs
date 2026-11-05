//! `expr` — integer arithmetic and comparisons.
//!
//! Grammar, lowest precedence first:
//!
//! ```text
//! expr    := compare
//! compare := additive (("=" | "!=" | "<" | "<=" | ">" | ">=") additive)?
//! additive:= multiply (("+" | "-") multiply)*
//! multiply:= unary (("*" | "/" | "%") unary)*
//! unary   := "!" unary | "-" unary | "+" unary | primary
//! primary := INTEGER | "(" expr ")"
//! ```
//!
//! All arithmetic is `i64` and checked: overflow and division by zero are
//! errors, never a panic. Comparisons and `!` produce `1`/`0`, and the printed
//! result decides the exit status: 0 for a non-zero result, 1 for zero, 2 for
//! a syntax/arithmetic error (`expr: syntax error`, `expr: division by zero`,
//! `expr: integer overflow` on fd 2). This port is integer-only; string
//! comparisons (`expr abc = abc`) are not implemented, and there is no `|`/`&`
//! or regex `:` operator. Tokens are separate arguments — the shell does not
//! expand `*` or `(`, so `expr 2 * 3` works as typed. No pipes/redirection.

use alloc::string::String;

use libpagh::eprintln;

use crate::util;

enum ExprError {
    Syntax,
    DivisionByZero,
    Overflow,
}

struct Parser<'a> {
    tokens: &'a [String],
    position: usize,
    depth: usize,
}

pub fn run(args: &[String]) -> i32 {
    let mut start = 0usize;
    if args.first().map(String::as_str) == Some("--") {
        start = 1;
    }
    let tokens = &args[start..];
    if tokens.is_empty() {
        eprintln!("expr: syntax error");
        return util::EXIT_USAGE;
    }

    let result = Parser {
        tokens,
        position: 0,
        depth: 0,
    }
    .parse();

    match result {
        Ok(value) => {
            let text = alloc::format!("{value}");
            if let Err(err) = util::write_line(1, text.as_bytes()) {
                eprintln!("expr: write error: {err}");
                return util::EXIT_FAILURE;
            }
            if value == 0 {
                util::EXIT_FAILURE
            } else {
                util::EXIT_OK
            }
        }
        Err(ExprError::Syntax) => {
            eprintln!("expr: syntax error");
            util::EXIT_USAGE
        }
        Err(ExprError::DivisionByZero) => {
            eprintln!("expr: division by zero");
            util::EXIT_USAGE
        }
        Err(ExprError::Overflow) => {
            eprintln!("expr: integer overflow");
            util::EXIT_USAGE
        }
    }
}

impl<'a> Parser<'a> {
    fn parse(mut self) -> Result<i64, ExprError> {
        let value = self.comparison()?;
        if self.position == self.tokens.len() {
            Ok(value)
        } else {
            Err(ExprError::Syntax)
        }
    }

    fn peek(&self) -> Option<&'a str> {
        self.tokens.get(self.position).map(String::as_str)
    }

    fn take(&mut self) -> Option<&'a str> {
        let token = self.peek()?;
        self.position += 1;
        Some(token)
    }

    fn comparison(&mut self) -> Result<i64, ExprError> {
        let left = self.additive()?;
        let Some(operator) = self.peek() else {
            return Ok(left);
        };
        if !matches!(operator, "=" | "!=" | "<" | "<=" | ">" | ">=") {
            return Ok(left);
        }
        self.position += 1;
        let right = self.additive()?;
        Ok(match operator {
            "=" => (left == right) as i64,
            "!=" => (left != right) as i64,
            "<" => (left < right) as i64,
            "<=" => (left <= right) as i64,
            ">" => (left > right) as i64,
            _ => (left >= right) as i64,
        })
    }

    fn additive(&mut self) -> Result<i64, ExprError> {
        let mut value = self.multiply()?;
        loop {
            let operator = match self.peek() {
                Some("+") => "+",
                Some("-") => "-",
                _ => return Ok(value),
            };
            self.position += 1;
            let right = self.multiply()?;
            value = match operator {
                "+" => value.checked_add(right).ok_or(ExprError::Overflow)?,
                _ => value.checked_sub(right).ok_or(ExprError::Overflow)?,
            };
        }
    }

    fn multiply(&mut self) -> Result<i64, ExprError> {
        let mut value = self.unary()?;
        loop {
            let operator = match self.peek() {
                Some("*") => "*",
                Some("/") => "/",
                Some("%") => "%",
                _ => return Ok(value),
            };
            self.position += 1;
            let right = self.unary()?;
            value = match operator {
                "*" => value.checked_mul(right).ok_or(ExprError::Overflow)?,
                "/" => {
                    if right == 0 {
                        return Err(ExprError::DivisionByZero);
                    }
                    value.checked_div(right).ok_or(ExprError::Overflow)?
                }
                _ => {
                    if right == 0 {
                        return Err(ExprError::DivisionByZero);
                    }
                    value.checked_rem(right).ok_or(ExprError::Overflow)?
                }
            };
        }
    }

    fn unary(&mut self) -> Result<i64, ExprError> {
        self.depth += 1;
        if self.depth > 128 {
            self.depth -= 1;
            return Err(ExprError::Syntax);
        }
        let result = match self.peek() {
            Some("!") => {
                self.position += 1;
                self.unary().map(|value| (value == 0) as i64)
            }
            Some("-") => {
                self.position += 1;
                self.unary()
                    .and_then(|value| value.checked_neg().ok_or(ExprError::Overflow))
            }
            Some("+") => {
                self.position += 1;
                self.unary()
            }
            _ => self.primary(),
        };
        self.depth -= 1;
        result
    }

    fn primary(&mut self) -> Result<i64, ExprError> {
        match self.take() {
            Some("(") => {
                let value = self.comparison()?;
                match self.take() {
                    Some(")") => Ok(value),
                    _ => Err(ExprError::Syntax),
                }
            }
            Some(token) => token.parse::<i64>().map_err(|_| ExprError::Syntax),
            None => Err(ExprError::Syntax),
        }
    }
}