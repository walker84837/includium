/// Check if a character can start an identifier (letter or underscore)
pub const fn is_identifier_start(c: char) -> bool {
    (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || c == '_'
}

/// Check if a character can continue an identifier (letter, digit, or underscore)
pub const fn is_identifier_continue(c: char) -> bool {
    (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') || (c >= '0' && c <= '9') || c == '_'
}

#[derive(Clone, Debug)]
pub(crate) enum Token {
    Identifier(String),
    StringLiteral(String),
    CharLiteral(String),
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ExprToken {
    Number(i64),
    Identifier(String),
    LParen,
    RParen,
    Not,
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    ShiftLeft,
    ShiftRight,
}
