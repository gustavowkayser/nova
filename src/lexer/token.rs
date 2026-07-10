use crate::lexer::token_types::TokenType;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]

pub struct Token {
    pub token_type: TokenType,
    pub value: Option<String>,
}

impl Token {
    pub fn new(token_type: TokenType, value: Option<String>) -> Self {
        Token {
            token_type,
            value
        }
    }
}