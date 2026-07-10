
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]

pub enum TokenType {
    BaseUrl,
    LBracket,
    RBracket,
    HttpMethod,
    // Add more token types as needed
}