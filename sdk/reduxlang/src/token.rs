#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Illegal(char),
    Eof,

    Let,
    Ident(String),
    Int(i64),

    Assign,
    Plus,
    Minus,
    Asterisk,
    Slash,

    LParen,
    RParen,
    Semicolon,
}
