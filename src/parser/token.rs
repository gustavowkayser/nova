#[derive(Debug, Clone, PartialEq, Eq)]

pub struct Token {
    pub token_type: String,
    pub value: String,
}

impl Token {
    pub fn new<T: Into<String>, V: Into<String>>(token_type: T, value: V) -> Self {
        Token {
            token_type: token_type.into(),
            value: value.into(),
        }
    }
}