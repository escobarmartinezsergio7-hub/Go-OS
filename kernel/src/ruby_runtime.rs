use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

const RUBY_RUNTIME_MAX_LOOP_ITERS: usize = 100_000;

#[derive(Clone)]
enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Nil,
}

impl Value {
    fn render(&self) -> String {
        match self {
            Value::Int(v) => alloc::format!("{}", v),
            Value::Float(v) => {
                let mut out = alloc::format!("{}", v);
                if out.contains('.') {
                    while out.ends_with('0') {
                        out.pop();
                    }
                    if out.ends_with('.') {
                        out.push('0');
                    }
                }
                out
            }
            Value::Bool(v) => {
                if *v {
                    String::from("true")
                } else {
                    String::from("false")
                }
            }
            Value::Str(v) => v.clone(),
            Value::Nil => String::from("nil"),
        }
    }

    fn truthy(&self) -> bool {
        match self {
            Value::Bool(v) => *v,
            Value::Nil => false,
            Value::Int(v) => *v != 0,
            Value::Float(v) => *v != 0.0,
            Value::Str(v) => !v.is_empty(),
        }
    }

    fn to_i64(&self) -> Option<i64> {
        match self {
            Value::Int(v) => Some(*v),
            Value::Float(v) => Some(*v as i64),
            Value::Bool(v) => Some(if *v { 1 } else { 0 }),
            Value::Str(v) => v.trim().parse::<i64>().ok(),
            Value::Nil => Some(0),
        }
    }

    fn to_f64(&self) -> Option<f64> {
        match self {
            Value::Int(v) => Some(*v as f64),
            Value::Float(v) => Some(*v),
            Value::Bool(v) => Some(if *v { 1.0 } else { 0.0 }),
            Value::Str(v) => v.trim().parse::<f64>().ok(),
            Value::Nil => Some(0.0),
        }
    }

    fn numeric_kind(&self) -> Option<bool> {
        match self {
            Value::Int(_) => Some(true),
            Value::Float(_) => Some(false),
            _ => None,
        }
    }
}

#[derive(Clone)]
enum TokenKind {
    Ident(String),
    Int(i64),
    Float(f64),
    Str(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    LParen,
    RParen,
    Equal,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
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
                b' ' | b'\t' | b'\r' | b'\0' => {
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
                    self.skip_hash_comment();
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
                    match self.peek_next() {
                        Some(b'/') => self.skip_slash_comment(),
                        Some(b'*') => self.skip_block_comment()?,
                        _ => {
                            tokens.push(Token {
                                kind: TokenKind::Slash,
                                line: self.line,
                            });
                            self.pos += 1;
                        }
                    }
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
                b',' => {
                    tokens.push(Token {
                        kind: TokenKind::Comma,
                        line: self.line,
                    });
                    self.pos += 1;
                }
                b'=' => {
                    let line = self.line;
                    self.pos += 1;
                    if self.peek() == Some(b'=') {
                        self.pos += 1;
                        tokens.push(Token {
                            kind: TokenKind::EqualEqual,
                            line,
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Equal,
                            line,
                        });
                    }
                }
                b'!' => {
                    let line = self.line;
                    self.pos += 1;
                    if self.peek() == Some(b'=') {
                        self.pos += 1;
                        tokens.push(Token {
                            kind: TokenKind::BangEqual,
                            line,
                        });
                    } else {
                        return Err(alloc::format!(
                            "line {}: unsupported token '!' (use '!=')",
                            line
                        ));
                    }
                }
                b'<' => {
                    let line = self.line;
                    self.pos += 1;
                    if self.peek() == Some(b'=') {
                        self.pos += 1;
                        tokens.push(Token {
                            kind: TokenKind::LessEqual,
                            line,
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Less,
                            line,
                        });
                    }
                }
                b'>' => {
                    let line = self.line;
                    self.pos += 1;
                    if self.peek() == Some(b'=') {
                        self.pos += 1;
                        tokens.push(Token {
                            kind: TokenKind::GreaterEqual,
                            line,
                        });
                    } else {
                        tokens.push(Token {
                            kind: TokenKind::Greater,
                            line,
                        });
                    }
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
                    let kind = self.read_number()?;
                    tokens.push(Token { kind, line });
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

    fn peek_next(&self) -> Option<u8> {
        let next = self.pos.saturating_add(1);
        if next >= self.src.len() {
            None
        } else {
            Some(self.src[next])
        }
    }

    fn skip_hash_comment(&mut self) {
        while let Some(ch) = self.peek() {
            self.pos += 1;
            if ch == b'\n' {
                self.line += 1;
                break;
            }
        }
    }

    fn skip_slash_comment(&mut self) {
        // consume "//"
        self.pos = self.pos.saturating_add(2);
        while let Some(ch) = self.peek() {
            self.pos += 1;
            if ch == b'\n' {
                self.line += 1;
                break;
            }
        }
    }

    fn skip_block_comment(&mut self) -> Result<(), String> {
        // consume "/*"
        self.pos = self.pos.saturating_add(2);
        while self.pos < self.src.len() {
            let ch = self.src[self.pos];
            if ch == b'\n' {
                self.line += 1;
            }
            if ch == b'*' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'/' {
                self.pos += 2;
                return Ok(());
            }
            self.pos += 1;
        }
        Err(alloc::format!(
            "line {}: unterminated block comment",
            self.line
        ))
    }

    fn read_number(&mut self) -> Result<TokenKind, String> {
        let start = self.pos;
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                self.pos += 1;
            } else {
                break;
            }
        }

        let mut is_float = false;
        if self.peek() == Some(b'.') {
            if self.pos + 1 < self.src.len() && self.src[self.pos + 1].is_ascii_digit() {
                is_float = true;
                self.pos += 1;
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
            }
        }

        let slice = &self.src[start..self.pos];
        let text = core::str::from_utf8(slice)
            .map_err(|_| alloc::format!("line {}: invalid number", self.line))?;

        if is_float {
            let parsed = text
                .parse::<f64>()
                .map_err(|_| alloc::format!("line {}: invalid float literal", self.line))?;
            Ok(TokenKind::Float(parsed))
        } else {
            let parsed = text
                .parse::<i64>()
                .map_err(|_| alloc::format!("line {}: integer overflow", self.line))?;
            Ok(TokenKind::Int(parsed))
        }
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

    fn run_program(&mut self) -> Result<(), String> {
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
        Ok(())
    }

    fn parse_statement(&mut self) -> Result<(), String> {
        if let Some(keyword) = self.peek_ident() {
            if keyword == "puts" || keyword == "log" {
                self.pos += 1;
                self.parse_puts()?;
                return Ok(());
            }
            if keyword == "print" {
                self.pos += 1;
                self.parse_print()?;
                return Ok(());
            }
            if keyword == "if" {
                self.pos += 1;
                self.parse_if_statement()?;
                return Ok(());
            }
            if keyword == "while" {
                self.pos += 1;
                self.parse_while_statement()?;
                return Ok(());
            }
            if keyword == "let" || keyword == "var" {
                self.pos += 1;
                self.parse_declaration_statement()?;
                return Ok(());
            }
            if keyword == "else" || keyword == "end" {
                return Err(alloc::format!("line {}: unexpected keyword '{}'", self.current_line(), keyword));
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

    fn parse_declaration_statement(&mut self) -> Result<(), String> {
        let name = match self.current().kind.clone() {
            TokenKind::Ident(id) => {
                if Self::is_reserved_keyword(id.as_str()) {
                    return Err(alloc::format!(
                        "line {}: invalid variable name '{}'",
                        self.current_line(),
                        id
                    ));
                }
                self.pos += 1;
                id
            }
            _ => {
                return Err(alloc::format!(
                    "line {}: expected identifier after declaration",
                    self.current_line()
                ))
            }
        };

        if !matches!(self.current().kind, TokenKind::Equal) {
            return Err(alloc::format!(
                "line {}: expected '=' after variable name",
                self.current_line()
            ));
        }
        self.pos += 1;
        let value = self.parse_expression()?;
        self.env.insert(name, value);
        Ok(())
    }

    fn parse_if_statement(&mut self) -> Result<(), String> {
        let cond = self.parse_expression()?;
        let _ = self.consume_keyword("then");
        self.skip_semicolons();

        let block_start = self.pos;
        let (else_idx, end_idx) = self.find_control_block_end(block_start, true)?;

        if cond.truthy() {
            self.execute_subprogram(block_start, else_idx.unwrap_or(end_idx))?;
        } else if let Some(else_pos) = else_idx {
            self.execute_subprogram(else_pos + 1, end_idx)?;
        }

        self.pos = end_idx + 1;
        Ok(())
    }

    fn parse_while_statement(&mut self) -> Result<(), String> {
        let cond_start = self.pos;
        let _ = self.parse_expression()?;
        let cond_end = self.pos;
        if cond_start >= cond_end {
            return Err(alloc::format!("line {}: empty while condition", self.current_line()));
        }

        let _ = self.consume_keyword("then");
        self.skip_semicolons();

        let body_start = self.pos;
        let (_, end_idx) = self.find_control_block_end(body_start, false)?;

        let mut loops = 0usize;
        loop {
            let cond_value = self.eval_expression_range(cond_start, cond_end)?;
            if !cond_value.truthy() {
                break;
            }
            self.execute_subprogram(body_start, end_idx)?;
            loops = loops.saturating_add(1);
            if loops > RUBY_RUNTIME_MAX_LOOP_ITERS {
                return Err(alloc::format!(
                    "line {}: while loop exceeded max iterations ({})",
                    self.current_line(),
                    RUBY_RUNTIME_MAX_LOOP_ITERS
                ));
            }
        }

        self.pos = end_idx + 1;
        Ok(())
    }

    fn find_control_block_end(
        &self,
        start: usize,
        allow_else: bool,
    ) -> Result<(Option<usize>, usize), String> {
        let mut idx = start;
        let mut depth = 0usize;
        let mut else_pos: Option<usize> = None;

        while idx < self.tokens.len() {
            match &self.tokens[idx].kind {
                TokenKind::Ident(name) => {
                    if name == "if" || name == "while" {
                        depth = depth.saturating_add(1);
                    } else if name == "end" {
                        if depth == 0 {
                            return Ok((else_pos, idx));
                        }
                        depth -= 1;
                    } else if allow_else && name == "else" && depth == 0 && else_pos.is_none() {
                        else_pos = Some(idx);
                    }
                }
                TokenKind::Eof => break,
                _ => {}
            }
            idx += 1;
        }

        Err(alloc::format!(
            "line {}: expected 'end' to close control block",
            self.current_line()
        ))
    }

    fn execute_subprogram(&mut self, start: usize, end: usize) -> Result<(), String> {
        if start >= end {
            return Ok(());
        }

        let mut sub_tokens: Vec<Token> = self.tokens[start..end].to_vec();
        let end_line = self
            .tokens
            .get(start)
            .map(|t| t.line)
            .unwrap_or_else(|| self.current_line());
        sub_tokens.push(Token {
            kind: TokenKind::Eof,
            line: end_line,
        });

        let mut child = Parser::new(sub_tokens);
        child.env = self.env.clone();
        child.run_program()?;

        self.env = child.env;
        self.output.extend(child.output.into_iter());
        Ok(())
    }

    fn eval_expression_range(&self, start: usize, end: usize) -> Result<Value, String> {
        if start >= end {
            return Err(alloc::format!(
                "line {}: expected expression",
                self.current_line()
            ));
        }

        let mut expr_tokens: Vec<Token> = self.tokens[start..end].to_vec();
        let line = self
            .tokens
            .get(start)
            .map(|t| t.line)
            .unwrap_or_else(|| self.current_line());
        expr_tokens.push(Token {
            kind: TokenKind::Eof,
            line,
        });

        let mut parser = Parser::new(expr_tokens);
        parser.env = self.env.clone();
        let value = parser.parse_expression()?;
        parser.skip_semicolons();
        if !parser.is_eof() {
            return Err(alloc::format!(
                "line {}: invalid expression in control condition",
                line
            ));
        }
        Ok(value)
    }

    fn parse_expression(&mut self) -> Result<Value, String> {
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> Result<Value, String> {
        let mut left = self.parse_add_sub()?;

        loop {
            if self.consume_equal_equal() {
                let right = self.parse_add_sub()?;
                left = Value::Bool(self.values_equal(&left, &right));
                continue;
            }
            if self.consume_bang_equal() {
                let right = self.parse_add_sub()?;
                left = Value::Bool(!self.values_equal(&left, &right));
                continue;
            }
            if self.consume_less() {
                let right = self.parse_add_sub()?;
                left = Value::Bool(self.compare_values(&left, &right)? == Ordering::Less);
                continue;
            }
            if self.consume_less_equal() {
                let right = self.parse_add_sub()?;
                let ord = self.compare_values(&left, &right)?;
                left = Value::Bool(ord == Ordering::Less || ord == Ordering::Equal);
                continue;
            }
            if self.consume_greater() {
                let right = self.parse_add_sub()?;
                left = Value::Bool(self.compare_values(&left, &right)? == Ordering::Greater);
                continue;
            }
            if self.consume_greater_equal() {
                let right = self.parse_add_sub()?;
                let ord = self.compare_values(&left, &right)?;
                left = Value::Bool(ord == Ordering::Greater || ord == Ordering::Equal);
                continue;
            }
            break;
        }

        Ok(left)
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
                Value::Float(v) => Ok(Value::Float(-v)),
                _ => Err(alloc::format!(
                    "line {}: unary '-' supports only numbers",
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
            TokenKind::Float(v) => {
                self.pos += 1;
                Ok(Value::Float(v))
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
                if name == "true" {
                    return Ok(Value::Bool(true));
                }
                if name == "false" {
                    return Ok(Value::Bool(false));
                }

                if matches!(self.current().kind, TokenKind::LParen) {
                    return self.parse_builtin_call(name.as_str());
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

    fn parse_builtin_call(&mut self, name: &str) -> Result<Value, String> {
        if !matches!(self.current().kind, TokenKind::LParen) {
            return Err(alloc::format!(
                "line {}: expected '(' after '{}'",
                self.current_line(),
                name
            ));
        }
        self.pos += 1;

        let mut args: Vec<Value> = Vec::new();
        if !self.consume_rparen() {
            loop {
                args.push(self.parse_expression()?);
                if self.consume_comma() {
                    continue;
                }
                if self.consume_rparen() {
                    break;
                }
                return Err(alloc::format!(
                    "line {}: expected ',' or ')' in call to {}",
                    self.current_line(),
                    name
                ));
            }
        }

        match name {
            "int" => {
                if args.len() != 1 {
                    return Err(alloc::format!("line {}: int(x) expects one argument", self.current_line()));
                }
                args[0]
                    .to_i64()
                    .map(Value::Int)
                    .ok_or_else(|| alloc::format!("line {}: cannot convert to int", self.current_line()))
            }
            "float" => {
                if args.len() != 1 {
                    return Err(alloc::format!("line {}: float(x) expects one argument", self.current_line()));
                }
                args[0]
                    .to_f64()
                    .map(Value::Float)
                    .ok_or_else(|| alloc::format!("line {}: cannot convert to float", self.current_line()))
            }
            "str" => {
                if args.len() != 1 {
                    return Err(alloc::format!("line {}: str(x) expects one argument", self.current_line()));
                }
                Ok(Value::Str(args[0].render()))
            }
            "bool" => {
                if args.len() != 1 {
                    return Err(alloc::format!("line {}: bool(x) expects one argument", self.current_line()));
                }
                Ok(Value::Bool(args[0].truthy()))
            }
            "len" => {
                if args.len() != 1 {
                    return Err(alloc::format!("line {}: len(x) expects one argument", self.current_line()));
                }
                let rendered = args[0].render();
                Ok(Value::Int(rendered.chars().count() as i64))
            }
            _ => Err(alloc::format!(
                "line {}: unsupported function '{}'",
                self.current_line(),
                name
            )),
        }
    }

    fn values_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Nil, Value::Nil) => true,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Int(a), Value::Float(b)) => (*a as f64) == *b,
            (Value::Float(a), Value::Int(b)) => *a == (*b as f64),
            (Value::Str(a), Value::Str(b)) => a == b,
            _ => left.render() == right.render(),
        }
    }

    fn compare_values(&self, left: &Value, right: &Value) -> Result<Ordering, String> {
        if left.numeric_kind().is_some() && right.numeric_kind().is_some() {
            let a = left
                .to_f64()
                .ok_or_else(|| alloc::format!("line {}: left value not numeric", self.current_line()))?;
            let b = right
                .to_f64()
                .ok_or_else(|| alloc::format!("line {}: right value not numeric", self.current_line()))?;
            return a
                .partial_cmp(&b)
                .ok_or_else(|| alloc::format!("line {}: cannot compare NaN", self.current_line()));
        }

        match (left, right) {
            (Value::Str(a), Value::Str(b)) => Ok(a.cmp(b)),
            (Value::Bool(a), Value::Bool(b)) => Ok(a.cmp(b)),
            _ => Err(alloc::format!(
                "line {}: comparison requires numeric, string, or bool values",
                self.current_line()
            )),
        }
    }

    fn apply_add(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_add(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            (a, b) => {
                if a.numeric_kind().is_some() && b.numeric_kind().is_some() {
                    let av = a
                        .to_f64()
                        .ok_or_else(|| alloc::format!("line {}: invalid numeric value", self.current_line()))?;
                    let bv = b
                        .to_f64()
                        .ok_or_else(|| alloc::format!("line {}: invalid numeric value", self.current_line()))?;
                    Ok(Value::Float(av + bv))
                } else {
                    let mut text = a.render();
                    text.push_str(b.render().as_str());
                    Ok(Value::Str(text))
                }
            }
        }
    }

    fn apply_sub(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_sub(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            (a, b) => {
                if a.numeric_kind().is_some() && b.numeric_kind().is_some() {
                    let av = a
                        .to_f64()
                        .ok_or_else(|| alloc::format!("line {}: invalid numeric value", self.current_line()))?;
                    let bv = b
                        .to_f64()
                        .ok_or_else(|| alloc::format!("line {}: invalid numeric value", self.current_line()))?;
                    Ok(Value::Float(av - bv))
                } else {
                    Err(alloc::format!(
                        "line {}: '-' supports only numbers",
                        self.current_line()
                    ))
                }
            }
        }
    }

    fn apply_mul(&self, left: Value, right: Value) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(a), Value::Int(b)) => a
                .checked_mul(b)
                .map(Value::Int)
                .ok_or_else(|| alloc::format!("line {}: integer overflow", self.current_line())),
            (a, b) => {
                if a.numeric_kind().is_some() && b.numeric_kind().is_some() {
                    let av = a
                        .to_f64()
                        .ok_or_else(|| alloc::format!("line {}: invalid numeric value", self.current_line()))?;
                    let bv = b
                        .to_f64()
                        .ok_or_else(|| alloc::format!("line {}: invalid numeric value", self.current_line()))?;
                    Ok(Value::Float(av * bv))
                } else {
                    Err(alloc::format!(
                        "line {}: '*' supports only numbers",
                        self.current_line()
                    ))
                }
            }
        }
    }

    fn apply_div(&self, left: Value, right: Value) -> Result<Value, String> {
        let a = left
            .to_f64()
            .ok_or_else(|| alloc::format!("line {}: '/' supports only numbers", self.current_line()))?;
        let b = right
            .to_f64()
            .ok_or_else(|| alloc::format!("line {}: '/' supports only numbers", self.current_line()))?;
        if b == 0.0 {
            return Err(alloc::format!("line {}: division by zero", self.current_line()));
        }
        Ok(Value::Float(a / b))
    }

    fn apply_mod(&self, left: Value, right: Value) -> Result<Value, String> {
        let a = left
            .to_f64()
            .ok_or_else(|| alloc::format!("line {}: '%' supports only numbers", self.current_line()))?;
        let b = right
            .to_f64()
            .ok_or_else(|| alloc::format!("line {}: '%' supports only numbers", self.current_line()))?;
        if b == 0.0 {
            return Err(alloc::format!("line {}: modulo by zero", self.current_line()));
        }
        Ok(Value::Float(a % b))
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
                if Self::is_reserved_keyword(name.as_str()) {
                    return None;
                }
                if matches!(self.peek_kind(1), TokenKind::Equal) {
                    Some(name.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn is_reserved_keyword(name: &str) -> bool {
        matches!(
            name,
            "puts" | "log" | "print" | "if" | "else" | "end" | "while" | "let" | "var" | "true" | "false" | "nil" | "then"
        )
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

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        match &self.current().kind {
            TokenKind::Ident(text) if text == keyword => {
                self.pos += 1;
                true
            }
            _ => false,
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

    fn consume_equal_equal(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::EqualEqual) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_bang_equal(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::BangEqual) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_less(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Less) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_less_equal(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::LessEqual) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_greater(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::Greater) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn consume_greater_equal(&mut self) -> bool {
        if matches!(self.current().kind, TokenKind::GreaterEqual) {
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
    let mut parser = Parser::new(tokens);
    parser.run_program()?;
    Ok(parser.output)
}
