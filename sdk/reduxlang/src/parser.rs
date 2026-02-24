use crate::ast::{Expr, Op, Statement};
use crate::lexer::Lexer;
use crate::token::Token;

#[derive(Debug)]
pub struct Parser {
    l: Lexer,
    cur: Token,
    peek: Token,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Precedence {
    Lowest,
    Sum,
    Product,
}

impl Parser {
    pub fn new(mut l: Lexer) -> Self {
        let cur = l.next_token();
        let peek = l.next_token();
        Self { l, cur, peek }
    }

    fn next_token(&mut self) {
        self.cur = core::mem::replace(&mut self.peek, self.l.next_token());
    }

    pub fn parse_program(&mut self) -> Result<Vec<Statement>, String> {
        let mut out = Vec::new();

        while self.cur != Token::Eof {
            let stmt = self.parse_statement()?;
            out.push(stmt);
            self.next_token();
        }

        Ok(out)
    }

    fn parse_statement(&mut self) -> Result<Statement, String> {
        match &self.cur {
            Token::Let => self.parse_let_statement(),
            _ => {
                let expr = self.parse_expression(Precedence::Lowest)?;
                if self.peek == Token::Semicolon {
                    self.next_token();
                }
                Ok(Statement::Expr(expr))
            }
        }
    }

    fn parse_let_statement(&mut self) -> Result<Statement, String> {
        self.next_token();
        let name = match &self.cur {
            Token::Ident(s) => s.clone(),
            other => return Err(format!("expected identifier, got {other:?}")),
        };

        self.next_token();
        if self.cur != Token::Assign {
            return Err("expected '=' after let identifier".to_string());
        }

        self.next_token();
        let value = self.parse_expression(Precedence::Lowest)?;

        if self.peek == Token::Semicolon {
            self.next_token();
        }

        Ok(Statement::Let(name, value))
    }

    fn parse_expression(&mut self, precedence: Precedence) -> Result<Expr, String> {
        let mut left = match &self.cur {
            Token::Int(v) => Expr::Int(*v),
            Token::Ident(name) => Expr::Ident(name.clone()),
            Token::LParen => {
                self.next_token();
                let expr = self.parse_expression(Precedence::Lowest)?;
                self.next_token();
                if self.cur != Token::RParen {
                    return Err("missing ')'".to_string());
                }
                expr
            }
            other => return Err(format!("unexpected token in expression: {other:?}")),
        };

        while self.peek != Token::Semicolon && precedence < token_precedence(&self.peek) {
            self.next_token();
            left = self.parse_infix_expression(left)?;
        }

        Ok(left)
    }

    fn parse_infix_expression(&mut self, left: Expr) -> Result<Expr, String> {
        let op = match self.cur {
            Token::Plus => Op::Add,
            Token::Minus => Op::Sub,
            Token::Asterisk => Op::Mul,
            Token::Slash => Op::Div,
            _ => return Err("invalid infix operator".to_string()),
        };

        let prec = token_precedence(&self.cur);
        self.next_token();
        let right = self.parse_expression(prec)?;

        Ok(Expr::Infix(Box::new(left), op, Box::new(right)))
    }
}

fn token_precedence(tok: &Token) -> Precedence {
    match tok {
        Token::Plus | Token::Minus => Precedence::Sum,
        Token::Asterisk | Token::Slash => Precedence::Product,
        _ => Precedence::Lowest,
    }
}
