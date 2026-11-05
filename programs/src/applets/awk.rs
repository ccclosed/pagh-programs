//! `awk` — a small field/record processor.
//!
//! This is a documented subset, not a POSIX awk:
//!
//! * `-F SEP` sets the field separator (default: runs of spaces/tabs). A
//!   separator may be several bytes; `\t`, `\n`, `\r` and `\\` are decoded.
//! * A rule is `PATTERN`, `{ ACTION }` or `PATTERN { ACTION }`.
//! * Patterns: `/RE/` (the same BRE-lite subset as `grep`/`sed`), `$0`, `$N`,
//!   `NR`, `NF`, a quoted string, a bare word, a decimal number, or two of the
//!   first five compared with `== != < <= > >=`.
//! * Actions: `print` with zero or more comma-separated operands, and several
//!   `print` statements separated by `;`. No variables, arrays, functions,
//!   arithmetic, assignments, `if`, loops, `printf`, `getline`, `BEGIN`/`END`
//!   or output redirection. A rule without an action prints `$0`.
//! * Bare words are compared as strings. In a full awk they would be variable
//!   names, but this subset has no variables, and the kernel shell cannot quote
//!   an argument containing spaces, so `$1==alpha` is the useful spelling
//!   here. Quoted strings work when the caller can pass quotes.
//!
//! Numeric comparisons are exact decimal comparisons: no floating point is
//! used, so `0.1 + ...` style arithmetic (which is not supported anyway) and
//! huge integers cannot silently round. `$N` past `NF` is the empty string.
//!
//! Exit: 0 success, 1 read/write/memory error, 2 usage or invalid program.

use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

use libpagh::eprintln;

use super::bre::Pattern;
use crate::util;

const USAGE: &str = "usage: awk [-F SEP] 'PROGRAM' [FILE...]";

#[derive(Clone, Copy)]
enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

enum Operand {
    Record,
    Field(usize),
    Nr,
    Nf,
    Value(Vec<u8>),
    Regex(Pattern),
}

enum Expr {
    Primary(Operand),
    Compare(Operand, Op, Operand),
}

enum Statement {
    Print(Vec<Operand>),
}

struct Rule {
    pattern: Option<Expr>,
    /// `None` is the default `{ print $0 }`; `Some(empty)` is `{}`.
    action: Option<Vec<Statement>>,
}

pub fn run(args: &[String]) -> i32 {
    let mut separator: Option<Vec<u8>> = None;
    let mut program: Option<&str> = None;
    let mut files: Vec<&str> = Vec::new();
    let mut parse_options = true;
    let mut index = 0usize;

    while index < args.len() {
        let arg = args[index].as_str();
        index += 1;
        if parse_options && arg == "--" {
            parse_options = false;
            continue;
        }
        if parse_options && arg == "-F" {
            let Some(value) = args.get(index).map(String::as_str) else {
                eprintln!("{USAGE}");
                return util::EXIT_USAGE;
            };
            index += 1;
            match decode_separator(value) {
                Some(decoded) => separator = Some(decoded),
                None => {
                    eprintln!("awk: invalid field separator: '{value}'");
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 2 && arg.starts_with("-F") {
            match decode_separator(&arg[2..]) {
                Some(decoded) => separator = Some(decoded),
                None => {
                    eprintln!("awk: invalid field separator: '{}'", &arg[2..]);
                    return util::EXIT_USAGE;
                }
            }
            continue;
        }
        if parse_options && arg.len() > 1 && arg.starts_with('-') {
            eprintln!("{USAGE}");
            return util::EXIT_USAGE;
        }

        parse_options = false;
        if program.is_none() {
            program = Some(arg);
        } else if util::push(&mut files, arg).is_err() {
            eprintln!("awk: out of memory");
            return util::EXIT_FAILURE;
        }
    }

    let Some(program) = program else {
        eprintln!("{USAGE}");
        return util::EXIT_USAGE;
    };
    let mut parser = Parser {
        bytes: program.as_bytes(),
        pos: 0,
    };
    let rules = match parser.parse_program() {
        Some(rules) if !rules.is_empty() => rules,
        _ => {
            eprintln!("awk: invalid program: '{program}'");
            return util::EXIT_USAGE;
        }
    };

    if files.is_empty() {
        files.push("-");
    }

    let mut status = util::EXIT_OK;
    let mut record_number = 0usize;

    for name in files {
        let data = match util::read_input(name) {
            Ok(data) => data,
            Err(err) => {
                util::report("awk", util::display_name(name), err);
                status = util::EXIT_FAILURE;
                continue;
            }
        };
        for line in util::lines(&data) {
            record_number = record_number.saturating_add(1);
            let fields = match split_fields(line, separator.as_deref()) {
                Some(fields) => fields,
                None => {
                    eprintln!("awk: out of memory");
                    return util::EXIT_FAILURE;
                }
            };
            let context = Context {
                line,
                fields,
                nr: record_number,
            };
            for rule in &rules {
                let selected = match &rule.pattern {
                    None => true,
                    Some(expr) => match evaluate(expr, &context) {
                        Some(value) => value,
                        None => {
                            eprintln!("awk: out of memory");
                            return util::EXIT_FAILURE;
                        }
                    },
                };
                if !selected {
                    continue;
                }
                match &rule.action {
                    None => {
                        if let Err(err) = util::write_line(1, line) {
                            eprintln!("awk: write error: {err}");
                            return util::EXIT_FAILURE;
                        }
                    }
                    Some(statements) => {
                        for statement in statements {
                            if let Err(error) = execute(statement, &context) {
                                return report_exec_error(error);
                            }
                        }
                    }
                }
            }
        }
    }

    status
}

struct Context<'a> {
    line: &'a [u8],
    fields: Vec<&'a [u8]>,
    nr: usize,
}

enum ExecError {
    Memory,
    Write(libpagh::Errno),
}

fn report_exec_error(error: ExecError) -> i32 {
    match error {
        ExecError::Memory => eprintln!("awk: out of memory"),
        ExecError::Write(err) => eprintln!("awk: write error: {err}"),
    }
    util::EXIT_FAILURE
}

fn execute(statement: &Statement, context: &Context<'_>) -> Result<(), ExecError> {
    match statement {
        Statement::Print(operands) => {
            if operands.is_empty() {
                return util::write_line(1, context.line).map_err(ExecError::Write);
            }
            let mut first = true;
            for operand in operands {
                let value = resolve(operand, context).ok_or(ExecError::Memory)?;
                if !first {
                    util::write_all(1, b" ").map_err(ExecError::Write)?;
                }
                util::write_all(1, value.as_bytes()).map_err(ExecError::Write)?;
                first = false;
            }
            util::write_line(1, b"").map_err(ExecError::Write)
        }
    }
}

fn evaluate(expr: &Expr, context: &Context<'_>) -> Option<bool> {
    match expr {
        Expr::Primary(operand) => match operand {
            Operand::Regex(pattern) => Some(pattern.is_match(context.line)),
            _ => {
                let value = resolve(operand, context)?;
                Some(truthy(value.as_bytes()))
            }
        },
        Expr::Compare(left, op, right) => {
            let left = resolve(left, context)?;
            let right = resolve(right, context)?;
            let ordering = compare(left.as_bytes(), right.as_bytes());
            Some(matches!(
                (op, ordering),
                (Op::Eq, Ordering::Equal)
                    | (Op::Ne, Ordering::Less | Ordering::Greater)
                    | (Op::Lt, Ordering::Less)
                    | (Op::Le, Ordering::Less | Ordering::Equal)
                    | (Op::Gt, Ordering::Greater)
                    | (Op::Ge, Ordering::Greater | Ordering::Equal)
            ))
        }
    }
}

enum Bytes<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl Bytes<'_> {
    fn as_bytes(&self) -> &[u8] {
        match self {
            Bytes::Borrowed(bytes) => bytes,
            Bytes::Owned(bytes) => bytes,
        }
    }
}

fn resolve<'a>(operand: &'a Operand, context: &'a Context<'a>) -> Option<Bytes<'a>> {
    match operand {
        Operand::Record => Some(Bytes::Borrowed(context.line)),
        Operand::Field(number) => {
            if *number == 0 {
                Some(Bytes::Borrowed(context.line))
            } else {
                match context.fields.get(number - 1) {
                    Some(field) => Some(Bytes::Borrowed(field)),
                    None => Some(Bytes::Borrowed(b"")),
                }
            }
        }
        Operand::Nr => Some(Bytes::Owned(decimal(context.nr))),
        Operand::Nf => Some(Bytes::Owned(decimal(context.fields.len()))),
        Operand::Value(value) => Some(Bytes::Borrowed(value)),
        Operand::Regex(_) => None,
    }
}

fn decimal(mut value: usize) -> Vec<u8> {
    let mut buffer = [0u8; 20];
    let mut position = buffer.len();
    loop {
        position -= 1;
        buffer[position] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    let mut out = Vec::new();
    let slice = &buffer[position..];
    if out.try_reserve(slice.len()).is_ok() {
        out.extend_from_slice(slice);
    }
    out
}

fn truthy(value: &[u8]) -> bool {
    match parse_decimal(value) {
        Some(number) => !number.is_zero(),
        None => !value.is_empty(),
    }
}

fn compare(left: &[u8], right: &[u8]) -> Ordering {
    match (parse_decimal(left), parse_decimal(right)) {
        (Some(left), Some(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

/// An exact decimal number, kept as sign and normalized digit slices.
struct Decimal<'a> {
    negative: bool,
    integer: &'a [u8],
    fraction: &'a [u8],
    zero: bool,
}

impl Decimal<'_> {
    fn is_zero(&self) -> bool {
        self.zero
    }
}

impl Ord for Decimal<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        if self.zero && other.zero {
            return Ordering::Equal;
        }
        if self.negative != other.negative {
            return if self.negative {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
        let magnitude = self
            .integer
            .len()
            .cmp(&other.integer.len())
            .then_with(|| self.integer.cmp(other.integer))
            .then_with(|| {
                let length = self.fraction.len().max(other.fraction.len());
                for index in 0..length {
                    let left = self.fraction.get(index).copied().unwrap_or(b'0');
                    let right = other.fraction.get(index).copied().unwrap_or(b'0');
                    match left.cmp(&right) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
                Ordering::Equal
            });
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for Decimal<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Decimal<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Decimal<'_> {}

/// Parse `[+-]?digits[.digits]?` exactly. Anything else (including exponents
/// and `inf`) is not numeric and is compared as a string.
fn parse_decimal(text: &[u8]) -> Option<Decimal<'_>> {
    if text.is_empty() {
        return None;
    }
    let mut index = 0usize;
    let mut negative = false;
    match text[0] {
        b'+' => index = 1,
        b'-' => {
            negative = true;
            index = 1;
        }
        _ => {}
    }
    let integer_start = index;
    while index < text.len() && text[index].is_ascii_digit() {
        index += 1;
    }
    let integer_end = index;
    let mut fraction_start = index;
    let mut fraction_end = index;
    if index < text.len() && text[index] == b'.' {
        index += 1;
        fraction_start = index;
        while index < text.len() && text[index].is_ascii_digit() {
            index += 1;
        }
        fraction_end = index;
    }
    if index != text.len() {
        return None;
    }
    if integer_end == integer_start && fraction_end == fraction_start {
        return None;
    }

    // Strip leading integer zeroes and trailing fractional zeroes so the
    // comparison is a plain length + lexicographic test.
    let mut integer = &text[integer_start..integer_end];
    while integer.len() > 1 && integer[0] == b'0' {
        integer = &integer[1..];
    }
    if integer == b"0" {
        integer = b"";
    }
    let mut fraction = &text[fraction_start..fraction_end];
    while fraction.last() == Some(&b'0') {
        fraction = &fraction[..fraction.len() - 1];
    }
    let zero = integer.is_empty() && fraction.is_empty();
    Some(Decimal {
        negative: negative && !zero,
        integer,
        fraction,
        zero,
    })
}

fn split_fields<'a>(line: &'a [u8], separator: Option<&[u8]>) -> Option<Vec<&'a [u8]>> {
    let mut fields: Vec<&'a [u8]> = Vec::new();
    match separator {
        None => {
            let mut index = 0usize;
            while index < line.len() {
                while index < line.len() && (line[index] == b' ' || line[index] == b'\t') {
                    index += 1;
                }
                if index >= line.len() {
                    break;
                }
                let start = index;
                while index < line.len() && line[index] != b' ' && line[index] != b'\t' {
                    index += 1;
                }
                fields.try_reserve(1).ok()?;
                fields.push(&line[start..index]);
            }
        }
        Some(separator) => {
            if separator.is_empty() {
                return None;
            }
            let mut start = 0usize;
            while let Some(offset) = find(&line[start..], separator) {
                fields.try_reserve(1).ok()?;
                fields.push(&line[start..start + offset]);
                start += offset + separator.len();
            }
            fields.try_reserve(1).ok()?;
            fields.push(&line[start..]);
        }
    }
    Some(fields)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    for start in 0..=haystack.len() - needle.len() {
        if &haystack[start..start + needle.len()] == needle {
            return Some(start);
        }
    }
    None
}

fn decode_separator(text: &str) -> Option<Vec<u8>> {
    let bytes = text.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut out: Vec<u8> = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        let decoded = if byte == b'\\' {
            match bytes.get(index) {
                Some(b't') => {
                    index += 1;
                    b'\t'
                }
                Some(b'n') => {
                    index += 1;
                    b'\n'
                }
                Some(b'r') => {
                    index += 1;
                    b'\r'
                }
                Some(b'\\') => {
                    index += 1;
                    b'\\'
                }
                _ => b'\\',
            }
        } else {
            byte
        };
        out.try_reserve(1).ok()?;
        out.push(decoded);
    }
    Some(out)
}

// ─── program parser ────────────────────────────────────────────────────────

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Parser<'a> {
    fn skip_space(&mut self) {
        while let Some(byte) = self.bytes.get(self.pos) {
            if matches!(byte, b' ' | b'\t' | b'\n' | b'\r') {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn parse_program(&mut self) -> Option<Vec<Rule>> {
        let mut rules: Vec<Rule> = Vec::new();
        loop {
            self.skip_space();
            if self.pos >= self.bytes.len() {
                break;
            }
            rules.try_reserve(1).ok()?;
            rules.push(self.parse_rule()?);
        }
        Some(rules)
    }

    fn parse_rule(&mut self) -> Option<Rule> {
        self.skip_space();
        let pattern = if self.peek() == Some(b'{') {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.skip_space();
        let action = if self.peek() == Some(b'{') {
            Some(self.parse_block()?)
        } else {
            None
        };
        Some(Rule { pattern, action })
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        let left = self.parse_operand()?;
        self.skip_space();
        let Some(op) = self.parse_op() else {
            return Some(Expr::Primary(left));
        };
        self.skip_space();
        let right = self.parse_operand()?;
        if matches!(left, Operand::Regex(_)) || matches!(right, Operand::Regex(_)) {
            return None;
        }
        Some(Expr::Compare(left, op, right))
    }

    fn parse_op(&mut self) -> Option<Op> {
        let rest = self.bytes.get(self.pos..)?;
        let (op, length) = if rest.starts_with(b"==") {
            (Op::Eq, 2)
        } else if rest.starts_with(b"!=") {
            (Op::Ne, 2)
        } else if rest.starts_with(b"<=") {
            (Op::Le, 2)
        } else if rest.starts_with(b">=") {
            (Op::Ge, 2)
        } else if rest.starts_with(b"<") {
            (Op::Lt, 1)
        } else if rest.starts_with(b">") {
            (Op::Gt, 1)
        } else {
            return None;
        };
        self.pos += length;
        Some(op)
    }

    fn parse_block(&mut self) -> Option<Vec<Statement>> {
        if self.peek() != Some(b'{') {
            return None;
        }
        self.pos += 1;
        let mut statements: Vec<Statement> = Vec::new();
        loop {
            self.skip_space();
            match self.peek()? {
                b'}' => {
                    self.pos += 1;
                    return Some(statements);
                }
                b';' => {
                    self.pos += 1;
                }
                _ => {
                    let statement = self.parse_statement()?;
                    statements.try_reserve(1).ok()?;
                    statements.push(statement);
                }
            }
        }
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        if !self.keyword(b"print") {
            return None;
        }
        self.skip_space();
        let mut operands: Vec<Operand> = Vec::new();
        if matches!(self.peek(), Some(b';') | Some(b'}')) {
            return Some(Statement::Print(operands));
        }
        loop {
            let operand = self.parse_operand()?;
            if matches!(operand, Operand::Regex(_)) {
                return None;
            }
            operands.try_reserve(1).ok()?;
            operands.push(operand);
            self.skip_space();
            match self.peek()? {
                b',' => {
                    self.pos += 1;
                    self.skip_space();
                }
                b';' | b'}' => break,
                _ => return None,
            }
        }
        Some(Statement::Print(operands))
    }

    fn keyword(&mut self, word: &[u8]) -> bool {
        let rest = &self.bytes[self.pos..];
        if !rest.starts_with(word) {
            return false;
        }
        match rest.get(word.len()) {
            Some(byte) if byte.is_ascii_alphanumeric() || *byte == b'_' => false,
            _ => {
                self.pos += word.len();
                true
            }
        }
    }

    fn parse_operand(&mut self) -> Option<Operand> {
        self.skip_space();
        match self.peek()? {
            b'$' => {
                self.pos += 1;
                let start = self.pos;
                while matches!(self.peek(), Some(byte) if byte.is_ascii_digit()) {
                    self.pos += 1;
                }
                if self.pos == start {
                    return None;
                }
                let mut number = 0usize;
                for byte in &self.bytes[start..self.pos] {
                    number = number.checked_mul(10)?.checked_add((*byte - b'0') as usize)?;
                }
                Some(if number == 0 {
                    Operand::Record
                } else {
                    Operand::Field(number)
                })
            }
            b'"' => {
                self.pos += 1;
                let mut value: Vec<u8> = Vec::new();
                loop {
                    let byte = self.bytes.get(self.pos).copied()?;
                    self.pos += 1;
                    match byte {
                        b'"' => return Some(Operand::Value(value)),
                        b'\\' => {
                            let next = self.bytes.get(self.pos).copied()?;
                            self.pos += 1;
                            let decoded = match next {
                                b'n' => b'\n',
                                b't' => b'\t',
                                b'r' => b'\r',
                                other => other,
                            };
                            value.try_reserve(1).ok()?;
                            value.push(decoded);
                        }
                        other => {
                            value.try_reserve(1).ok()?;
                            value.push(other);
                        }
                    }
                }
            }
            b'/' => {
                self.pos += 1;
                let mut pattern: Vec<u8> = Vec::new();
                loop {
                    let byte = self.bytes.get(self.pos).copied()?;
                    self.pos += 1;
                    match byte {
                        b'/' => break,
                        b'\\' => {
                            let next = self.bytes.get(self.pos).copied()?;
                            self.pos += 1;
                            pattern.try_reserve(2).ok()?;
                            pattern.push(b'\\');
                            pattern.push(next);
                        }
                        other => {
                            pattern.try_reserve(1).ok()?;
                            pattern.push(other);
                        }
                    }
                }
                Some(Operand::Regex(Pattern::compile(&pattern)?))
            }
            _ => {
                let start = self.pos;
                while let Some(byte) = self.peek() {
                    if byte.is_ascii_whitespace()
                        || matches!(byte, b',' | b';' | b'}' | b'{' | b'<' | b'>' | b'=' | b'!')
                    {
                        break;
                    }
                    self.pos += 1;
                }
                if self.pos == start {
                    return None;
                }
                let token = &self.bytes[start..self.pos];
                if token == b"BEGIN" || token == b"END" {
                    // Not implemented; refuse instead of silently treating the
                    // keyword as a truthy string pattern.
                    None
                } else if token == b"NR" {
                    Some(Operand::Nr)
                } else if token == b"NF" {
                    Some(Operand::Nf)
                } else {
                    let mut value: Vec<u8> = Vec::new();
                    value.try_reserve(token.len()).ok()?;
                    value.extend_from_slice(token);
                    Some(Operand::Value(value))
                }
            }
        }
    }
}
