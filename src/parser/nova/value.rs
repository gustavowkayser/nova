use crate::parser::nova::ast::Reference;

/// The value model for Nova request bodies: JSON, plus references.
#[derive(Debug, Clone, PartialEq)]
pub enum NovaValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<NovaValue>),
    Object(Vec<(String, NovaValue)>),
    Ref(Reference),
}
