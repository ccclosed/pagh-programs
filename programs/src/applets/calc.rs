//! `calc` — evaluate integer arithmetic expressions.
//!
//! Grammar, lowest precedence first:
//!
//! ```text
//! expression := additive
//! additive   := multiplicative (("+" | "-") multiplicative)*
//! multiplicative := unary (("*" | "/" | "%") unary)*
//! unary      := ("+" | "-") unary | primary
//! primary    := INTEGER | "(" expression ")"
//! ```
//!
//! `INTEGER` is a decimal `i64`; whitespace between tokens is ignored. All
//! arithmetic is checked: `+`, `-` and `*` report an overflow instead of
//! wrapping. Every operand is printed on its own line, in argument order.
//!
//! ## Command line
//!
//! ```text
//! calc [-e EXPR] [EXPR...]
//! ```
//!
//! Each positional operand (and each `-e`/`--eval` operand) is one complete
//! expression; the kernel shell splits on spaces and has no quoting, so an
//! expression is normally a single word (`calc 2+3*4`, `calc '1 + 2'` under a
//! POSIX shell). `--` ends option parsing, so an expression that begins with
//! `-` can always be written after it (`calc -- -5+2`), though a leading `-`
//! operand is treated as an expression anyway.
//!
//! Exit: 0 all expressions evaluated; 1 division (or remainder) by zero or an
//! `i64` overflow; 2 a syntax error or an empty operand list. A failing
//! expression stops the run; earlier results are already on stdout.

use alloc::string::String;
use alloc::vec::Vec;

use libpagh::eprintln;

use crate::util;

const USAGE: &str = "usage: calc [-e EXPR] [EXPR...]";
/// Deepest allowed parenthesis nesting. A bounded parser cannot exhaust the
/// process stack on a hostile operand.
const MAX_DEPTH: usize = 64;

enum CalcError {
    Syntax,
    DivisionByZero,
    Overflow,
}

pub fn run(args: &[String]) -> i32 {
    let mut expressions: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && (arg == "-e" || arg == "--eval") {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            if util::push(&mut expressions, value).is_err() {
                eprintln!("calc: out of memory");
                return util::EXIT_FAILURE;
            }
            continue;
        }
        // Every other operand is an expression, including one that starts
        // with '-' (there is no option that could be confused with a
        // negative number unless it is `-e` itself).
        if util::push(&mut expressions, arg).is_err() {
            eprintln!("calc: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    if expressions.is_empty() {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    }

    for expression in expressions {
        match evaluate(expression) {
            Ok(value) => {
                let text = alloc::format!("{value}");
                if let Err(err) = util::write_line(1, text.as_bytes()) {
                    eprintln!("calc: write error: {err}");
                    return util::EXIT_FAILURE;
                }
            }
            Err(CalcError::Syntax) => {
                eprintln!("calc: syntax error");
                return util::EXIT_USAGE;
            }
            Err(CalcError::DivisionByZero) => {
                eprintln!("calc: division by zero");
                return util::EXIT_FAILURE;
            }
            Err(CalcError::Overflow) => {
                eprintln!("calc: integer overflow");
                return util::EXIT_FAILURE;
            }
        }
    }
    util::EXIT_OK
}

fn evaluate(text: &str) -> Result<i64, CalcError> {
    let mut parser = Parser {
        text: text.as_bytes(),
        position: 0,
        depth: 0,
    };
    let value = parser.expression()?;
    parser.skip_space();
    if parser.position != parser.text.len() {
        return Err(CalcError::Syntax);
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a [u8],
    position: usize,
    depth: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.position).copied()
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.position += 1;
        }
    }

    fn expression(&mut self) -> Result<i64, CalcError> {
        let mut value = self.multiplicative()?;
        loop {
            self.skip_space();
            let operator = match self.peek() {
                Some(b'+') => b'+',
                Some(b'-') => b'-',
                _ => return Ok(value),
            };
            self.position += 1;
            let right = self.multiplicative()?;
            value = match operator {
                b'+' => value.checked_add(right).ok_or(CalcError::Overflow)?,
                _ => value.checked_sub(right).ok_or(CalcError::Overflow)?,
            };
        }
    }

    fn multiplicative(&mut self) -> Result<i64, CalcError> {
        let mut value = self.unary()?;
        loop {
            self.skip_space();
            let operator = match self.peek() {
                Some(b'*') => b'*',
                Some(b'/') => b'/',
                Some(b'%') => b'%',
                _ => return Ok(value),
            };
            self.position += 1;
            let right = self.unary()?;
            value = match operator {
                b'*' => value.checked_mul(right).ok_or(CalcError::Overflow)?,
                b'/' => {
                    if right == 0 {
                        return Err(CalcError::DivisionByZero);
                    }
                    value.checked_div(right).ok_or(CalcError::Overflow)?
                }
                _ => {
                    if right == 0 {
                        return Err(CalcError::DivisionByZero);
                    }
                    value.checked_rem(right).ok_or(CalcError::Overflow)?
                }
            };
        }
    }

    fn unary(&mut self) -> Result<i64, CalcError> {
        self.skip_space();
        match self.peek() {
            Some(b'+') => {
                self.position += 1;
                self.unary()
            }
            Some(b'-') => {
                self.position += 1;
                let value = self.unary()?;
                value.checked_neg().ok_or(CalcError::Overflow)
            }
            _ => self.primary(),
        }
    }

    fn primary(&mut self) -> Result<i64, CalcError> {
        self.skip_space();
        match self.peek() {
            Some(b'(') => {
                if self.depth >= MAX_DEPTH {
                    return Err(CalcError::Syntax);
                }
                self.position += 1;
                self.depth += 1;
                let value = self.expression()?;
                self.depth -= 1;
                self.skip_space();
                if self.peek() != Some(b')') {
                    return Err(CalcError::Syntax);
                }
                self.position += 1;
                Ok(value)
            }
            Some(byte) if byte.is_ascii_digit() => self.number(),
            _ => Err(CalcError::Syntax),
        }
    }

    fn number(&mut self) -> Result<i64, CalcError> {
        let mut value: i64 = 0;
        let mut digits = 0usize;
        while let Some(byte) = self.peek() {
            if !byte.is_ascii_digit() {
                break;
            }
            let digit = i64::from(byte - b'0');
            value = value
                .checked_mul(10)
                .and_then(|value| value.checked_add(digit))
                .ok_or(CalcError::Overflow)?;
            digits += 1;
            self.position += 1;
        }
        if digits == 0 {
            return Err(CalcError::Syntax);
        }
        Ok(value)
    }
}