use crate::parser::nova::value::NovaValue;

/// A parsed `.nova` file: statements in source order.
///
/// The parser deliberately does not resolve anything. `@host` and `@header`
/// apply to the requests below them, but folding that context in — and
/// deciding what any given reference points at — is the execution engine's
/// job, not the parser's.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    pub kind: StatementKind,
    /// Byte offset of the statement's first character within the source.
    /// Pair with [`super::line_column`] to report a location.
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StatementKind {
    /// `@host` followed by a URL line.
    Host(Template),
    /// `@header` followed by `Name: Value` lines.
    Headers(Vec<Header>),
    Request(Request),
    /// `@token = @login.response.body.accessToken`
    Assign(Assign),
    Assert(Assert),
    /// `#command auth email password`
    Command(Command),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    /// `@login` — absent for an unlabelled request.
    pub name: Option<String>,
    /// The tag in `@login.auth`, naming the command this request belongs to.
    pub command: Option<String>,
    pub method: Method,
    pub path: Template,
    pub body: Option<NovaValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub name: String,
    pub value: Template,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assign {
    pub name: String,
    pub value: Reference,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assert {
    pub request: String,
    pub assertion: Assertion,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub name: String,
    pub parameters: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Assertion {
    /// `accessToken: string`
    TypeOnly(Vec<(String, TypeName)>),
    /// `[ "email" ]`
    HasField(Vec<String>),
    /// `[ "email", "user_id" ]`
    ExactFields(Vec<String>),
    /// `email: "john@example.com"`
    FieldMatch(Vec<(String, NovaValue)>),
    ExactMatch(Vec<(String, NovaValue)>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeName {
    String,
    Number,
    Boolean,
    Array,
    Object,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// `@env.EMAIL`, `@accessToken`, `@login.response.body.accessToken`,
/// `#auth.email`.
///
/// Structural only: the parser records the sigil and the dotted segments and
/// makes no claim about what they refer to. Telling an environment lookup
/// from a request path needs a symbol table, which is resolution, not syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reference {
    pub sigil: Sigil,
    /// Always at least one segment.
    pub segments: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sigil {
    At,
    Hash,
}

/// Line text that interleaves literal runs with references, as in
/// `Bearer @accessToken`.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub parts: Vec<TemplatePart>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Literal(String),
    Ref(Reference),
}
