use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone)]
enum Value {
    Int(i64),
    Str(String),
    Nil,
}

impl Value {
    fn render(&self) -> String {
        match self {
            Value::Int(v) => alloc::format!("{}", v),
            Value::Str(v) => v.clone(),
            Value::Nil => String::from("nil"),
        }
    }
}

#[derive(Clone)]
enum TokenKind {
    Ident(String),
    Int(i64),
    Str(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Equal,
    Comma,
    Semicolon,
    Eof,
}

#[derive(Clone)]
struct Token {
    kind: TokenKind,
    line: usize,
}

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
    line: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            src: input.as_bytes(),
            pos: 0,
            line: 1,
        }
    }

    fn tokenize(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();

        while let Some(ch) = self.peek() {
            match ch {
                b' ' | b'\t' | b'\r' => {
                    self.pos += 1;
                }
                b'\n' | b';' => {
                    tokens.push(Token {
                        kind: TokenKind::Semicolon,
                        line: self.line,
                    });
                    self.pos += 1;
                    if ch == b'\n' {
                        self.line += 1;
                    }
                }
                b'#' => {
                    self.skip_comment();
                }
                b'+' => {
                    tokens.push(Token {
                        kind: TokenKind::Plus,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'-' => {
                    tokens.push(Token {
                        kind: TokenKind::Minus,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'*' => {
                    tokens.push(Token {
                        kind: TokenKind::Star,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'/' => {
                    tokens.push(Token {
                        kind: TokenKind::Slash,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'%' => {
                    tokens.push(Token {
                        kind: TokenKind::Percent,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'(' => {
                    tokens.push(Token {
                        kind: TokenKind::LParen,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b')' => {
                    tokens.push(Token {
                        kind: TokenKind::RParen,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'=' => {
                    tokens.push(Token {
                        kind: TokenKind::Equal,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b',' => {
                    tokens.push(Token {
                        kind: TokenKind::Comma,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'"' | b'\'' => {
                    let line = self.line;
                    let text = self.read_string(ch)?;
                    tokens.push(Token {
                        kind: TokenKind::Str(text),
                        line,
                    });
                }
                b'0'..=b'9' => {
                    let line = self.line;
                    let v = self.read_int()?;
                    tokens.push(Token {
                        kind: TokenKind::Int(v),
                        line,
                    });
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                    let line = self.line;
                    let ident = self.read_ident();
                    tokens.push(Token {
                        kind: TokenKind::Ident(ident),
                        line,
                    });
                }
                _ => {
                    return Err(alloc::format!(
                        "line {}: unsupported character '{}'",
                        self.line,
                        ch as char
                    ));
                }
            }
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            line: self.line,
        });

        Ok(tokens)
    }

    fn peek(&self) -> Option<u8> {
        if self.pos >= self.src.len() {
            None
        } else {
            Some(self.src[self.pos])
        }
    }

    fn skip_comment(&mut self) {
        while let Some(ch) = self.peek() {
            self.pos += 1;
            if ch == b'\n' {
                self.line += 1;
                break;
            }
        }
    }

    fn read_int(&mut self) -> Result<i64, String> {
        let mut value: i64 = 0;
        let mut has_digits = false;

        while let Some(ch) = self.peek() {
            if !ch.is_ascii_digit() {
                break;
            }
            has_digits = true;
            let digit = (ch - b'0') as i64;
            value = value
                .checked_mul(10)
                .and_then(|v| v.checked_add(digit))
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.line))?;
            self.pos += 1;
        }

        if !has_digits {
            return Err(alloc::format!("line {}: expected integer", self.line));
        }

        Ok(value)
    }

    fn read_ident(&mut self) -> String {
        let start = self.pos;
        self.pos += 1;

        while let Some(ch) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }

        let slice = &self.src[start..self.pos];
        match core::str::from_utf8(slice) {
            Ok(text) => String::from(text),
            Err(_) => String::new(),
        }
    }

    fn read_string(&mut self, quote: u8) -> Result<String, String> {
        let mut out = String::new();
        self.pos += 1;

        while let Some(ch) = self.peek() {
            self.pos += 1;
            if ch == quote {
                return Ok(out);
            }

            if ch == b'\\' {
                let esc = self
                    .peek()
                    .ok_or_else(|| alloc::format!("line {}: unterminated escape", self.line))?;
                self.pos += 1;
                match esc {
                    b'n' => out.push('\n'),
                    b't' => out.push('\t'),
                    b'r' => out.push('\r'),
                    b'\\' => out.push('\\'),
                    b'"' => out.push('"'),
                    b'\'' => out.push('\''),
                    _ => out.push(esc as char),
                }
                continue;
            }

            if ch == b'\n' {
                self.line += 1;
            }
            out.push(ch as char);
        }

        Err(alloc::format!("line {}: unterminated string", self.line))
    }
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    env: BTreeMap<String, Value>,
    output: Vec<String>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            env: BTreeMap::new(),
            output: Vec::new(),
        }
    }

    fn run(mut self) -> Result<Vec<String>, String> {
        while !self.is_eof() {
            self.skip_semicolons();
            if self.is_eof() {
                break;
            }
            self.parse_statement()?;
            if !self.is_stmt_end() {
                return Err(alloc::format!(
                    "line {}: expected ';' or newline",
                    self.current_line()
                ));
            }
            self.skip_semicolons();
        }

        Ok(self.output)
    }

    fn parse_statement(&mut self) -> Result<(), String> {
        if let Some(keyword) = self.peek_ident() {
            if keyword == "puts" {
                self.pos += 1;
                self.parse_puts()?;
                return Ok(());
            }
            if keyword == "print" {
                self.pos += 1;
                self.parse_print()?;
                return Ok(());
            }
        }

        if let Some(name) = self.peek_assignment_target() {
            self.pos += 2;
            let value = self.parse_expression()?;
            self.env.insert(name, value);
            return Ok(());
        }

        let value = self.parse_expression()?;
        self.output.push(alloc::format!("=> {}", value.render()));
        Ok(())
    }

    fn parse_puts(&mut self) -> Result<(), String> {
        if self.is_stmt_end() {
            self.output.push(String::new());
            return Ok(());
        }

        loop {
            let value = self.parse_expression()?;
            self.output.push(value.render());
            if !self.consume_comma() {
                break;
            }
        }

        Ok(())
    }

    fn parse_print(&mut self) -> Result<(), String> {
        if self.is_stmt_end() {
            return Ok(());
        }

        let mut text = String::new();
        loop {
            let value = self.parse_expression()?;
            text.push_str(value.render().as_str());
            if !self.consume_comma() {
                break;
            }
        }
        self.output.push(text);
        Ok(())
    }

    fn parse_expression(&mut self) -> Result<Value, String> {
        self.parse_add_sub()
    }

    fn parse_add_sub(&mut self) -> Result<Value, String> {
        let mut left = self.parse_mul_div()?;

        loop {
            if self.consume_plus() {
                let right = self.parse_mul_div()?;
                left = self.apply_add(left, right)?;
                continue;
            }
            if self.consume_minus() {
                let right = self.parse_mul_div()?;
                left = self.apply_sub(left, right)?;
                continue;
            }
            break;
        }

        Ok(left)
    }

    fn parse_mul_div(&mut self) -> Result<Value, String> {
        let mut left = self.parse_unary()?;

        loop {
            if self.consume_star() {
                let right = self.parse_unary()?;
                left = self.apply_mul(left, right)?;
                continue;
            }
            if self.consume_slash() {
                let right = self.parse_unary()?;
                left = self.apply_div(left, right)?;
                continue;
            }
            if self.consume_percent() {
                let right = self.parse_unary()?;
                left = self.apply_mod(left, right)?;
                continue;
            }
            break;
        }

        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Value, String> {
        if self.consume_minus() {
            let value = self.parse_unary()?;
            match value {
                Value::Int(v) => v
                    .checked_neg()
                    .map(Value::Int)
                    .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
                _ => Err(alloc::format!(
                    "line {}: unary '-' only supports integers",
                    self.current_line()
                )),
            }
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Value, String> {
        if self.is_eof() {
            return Err(alloc::format!(
                "line {}: unexpected end of expression",
                self.current_line()
            ));
        }

        let token = self.current().clone();
        match token.kind {
            TokenKind::Int(v) => {
                self.pos += 1;
                Ok(Value::Int(v))
            }
            TokenKind::Str(s) => {
                self.pos += 1;
                Ok(Value::Str(s))
            }
            TokenKind::Ident(name) => {
                self.pos += 1;
                if name == "nil" {
                    return Ok(Value::Nil);
                }

                match self.env.get(&name) {
                    Some(v) => Ok(v.clone()),
                    None => Err(alloc::format!(
                        "line {}: undefined local variable '{}'",
                        token.line,
                        name
                    )),
                }
            }
            TokenKind::LParen => {
                self.pos += 1;
                let value = self.parse_expression()?;
                if !self.consume_rparen() {
                    return Err(alloc::format!("line {}: expected ')'", token.line));
                }
                Ok(value)
            }
            _ => Err(alloc::format!("line {}: invalid expression", token.line)),
        }
    }

    fn apply_add(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_add(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            (a, b) => {
                let mut text = a.render();
                text.push_str(b.render().as_str());
                Ok(Value::Str(text))
            }
        }
    }

    fn apply_sub(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_sub(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            _ => Err(alloc::format!(
                "line {}: '-' only supports integers",
                self.current_line()
            )),
        }
    }

    fn apply_mul(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_mul(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            _ => Err(alloc::format!(
                "line {}: '*' only supports integers",
                self.current_line()
            )),
        }
    }

    fn apply_div(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(_), Value::Int(0)) => {
                Err(alloc::format!("line {}: division by zero", self.current_line()))
            }
            (Value::Int(a), Value::Int(b)) => a
                .checked_div(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            _ => Err(alloc::format!(
                "line {}: '/' only supports integers",
                self.current_line()
            )),
        }
    }

    fn apply_mod(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(_), Value::Int(0)) => {
                Err(alloc::format!("line {}: modulo by zero", self.current_line()))
            }
            (Value::Int(a), Value::Int(b)) => a
                .checked_rem(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            _ => Err(alloc::format!(
                "line {}: '%' only supports integers",
                self.current_line()
            )),
        }
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn current_line(&self) -> usize {
        self.current().line
    }

    fn peek_ident(&self) -> Option<&str> {
        match &self.current().kind {
            TokenKind::Ident(text) => Some(text.as_str()),
            _ => None,
        }
    }

    fn peek_assignment_target(&self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Ident(name) => {
                if matches!(self.peek_kind(1), TokenKind::Equal) {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn peek_kind(&self, offset: usize) -> &TokenKind {
        let idx = (self.pos + offset).min(self.tokens.len() - 1);
        &self.tokens[idx].kind
    }

    fn is_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn is_stmt_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::Semicolon | TokenKind::Eof)
    }

    fn skip_semicolons(&mut self) {
        while matches!(self.current().kind, TokenKind::Semicolon) {
            self.pos += 1;
        }
    }

    fn consume_plus(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Plus) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_minus(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Minus) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_star(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Star) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_slash(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Slash) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_percent(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Percent) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_comma(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Comma) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_rparen(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::RParen) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
}

pub fn eval(script: &str) -> Result<Vec<String>, String> {
    let tokens = Lexer::new(script).tokenize()?;
    Parser::new(tokens).run()
}
