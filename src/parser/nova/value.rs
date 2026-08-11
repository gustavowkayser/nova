use crate::parser::json_parser;
use crate::parser::nova::ast::Reference;
use crate::parser::nova::lex;
use crate::parser::parser::Parser;

/// The value model for Nova request bodies: JSON, plus references.
///
/// This is deliberately not `JsonValue`. Nova bodies admit references and
/// trailing commas; `json_parser` stays a strict RFC 8259 parser because it
/// also has to read real HTTP responses.
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

/// A request body. Objects and arrays only, which makes "is there a body
/// here?" a single-character decision for the caller.
pub fn body() -> Parser<NovaValue> {
    return whitespace().ignore_left(object().or(array()));
}

/// A value on a single line, leaving the line ending for the caller. Used by
/// the `fieldMatch` and `exactMatch` assertion members.
pub fn line_value() -> Parser<NovaValue> {
    return Parser::<NovaValue>::new(|input| value_core().parse(input));
}

/// `[ "email", "user_id" ]` — the inline form used by `hasField` and
/// `exactFields`. The closing bracket is deliberately not tokenized, so the
/// newline after it survives for `eol` to consume.
pub fn string_array() -> Parser<Vec<String>> {
    let comma = token(Parser::<char>::char(','));
    let strings = token(json_parser::string_literal())
        .sep_by1(comma.clone())
        .ignore_right(Parser::<char>::opt(comma));
    let elements = strings.or(Parser::<Vec<String>>::returnp(Vec::new()));

    return token(Parser::<char>::char('['))
        .ignore_left(elements)
        .ignore_right(Parser::<char>::char(']'))
        .label("a list of field names");
}

/// A value, plus any whitespace after it.
///
/// The recursion knot: the parser tree is only built once parsing reaches
/// here, so `array` and `object` can refer back without building an
/// infinitely deep parser up front.
fn value() -> Parser<NovaValue> {
    return Parser::<NovaValue>::new(|input| token(value_core()).parse(input));
}

fn value_core() -> Parser<NovaValue> {
    let alternatives = vec![
        null(),
        boolean(),
        string_value(),
        number_value(),
        reference_value(),
        array(),
        object(),
    ];

    return Parser::<NovaValue>::choice(alternatives).label("a value");
}

fn null() -> Parser<NovaValue> {
    return json_parser::keyword("null").apply_return(NovaValue::Null);
}

fn boolean() -> Parser<NovaValue> {
    let yes = json_parser::keyword("true").apply_return(NovaValue::Bool(true));
    let no = json_parser::keyword("false").apply_return(NovaValue::Bool(false));

    return yes.or(no);
}

fn string_value() -> Parser<NovaValue> {
    return json_parser::string_literal().map(NovaValue::String);
}

fn number_value() -> Parser<NovaValue> {
    return json_parser::number().map(NovaValue::Number);
}

fn reference_value() -> Parser<NovaValue> {
    return lex::reference().map(NovaValue::Ref);
}

fn array() -> Parser<NovaValue> {
    let comma = token(Parser::<char>::char(','));
    let filled = value()
        .sep_by1(comma.clone())
        .ignore_right(Parser::<char>::opt(comma));
    let elements = filled.or(Parser::<Vec<NovaValue>>::returnp(Vec::new()));

    return token(Parser::<char>::char('['))
        .ignore_left(elements)
        .ignore_right(Parser::<char>::char(']'))
        .map(NovaValue::Array);
}

fn object() -> Parser<NovaValue> {
    let comma = token(Parser::<char>::char(','));
    let filled = member()
        .sep_by1(comma.clone())
        .ignore_right(Parser::<char>::opt(comma));
    let members = filled.or(Parser::<Vec<(String, NovaValue)>>::returnp(Vec::new()));

    return token(Parser::<char>::char('{'))
        .ignore_left(members)
        .ignore_right(Parser::<char>::char('}'))
        .map(NovaValue::Object);
}

fn member() -> Parser<(String, NovaValue)> {
    return token(json_parser::string_literal())
        .ignore_right(token(Parser::<char>::char(':')))
        .then(value());
}

fn token<T>(parser: Parser<T>) -> Parser<T>
where
    T: 'static,
{
    return parser.ignore_right(whitespace());
}

/// Whitespace inside a body, where newlines are insignificant and comments
/// are allowed — commenting out a field is exactly when you want them.
///
/// This is why `json_parser`'s own whitespace helper is not reused: that one
/// knows nothing about comments.
fn whitespace() -> Parser<()> {
    let space = Parser::<char>::any(" \t\r\n".chars()).apply_return(());

    return space.or(lex::comment()).many().apply_return(());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::nova::ast::Sigil;

    fn parse(input: &str) -> NovaValue {
        let (value, remaining) = body().parse(input).expect("body should parse");
        assert_eq!(remaining.trim(), "", "body left unparsed input");
        return value;
    }

    fn string(text: &str) -> NovaValue {
        NovaValue::String(text.to_string())
    }

    #[test]
    fn parses_an_ordinary_json_object() {
        assert_eq!(
            parse(r#"{ "email": "john@example.com", "age": 30 }"#),
            NovaValue::Object(vec![
                ("email".to_string(), string("john@example.com")),
                ("age".to_string(), NovaValue::Number(30.0)),
            ])
        );
    }

    #[test]
    fn parses_literals_and_nesting() {
        assert_eq!(
            parse(r#"{ "a": null, "b": true, "c": [1, [2]], "d": {} }"#),
            NovaValue::Object(vec![
                ("a".to_string(), NovaValue::Null),
                ("b".to_string(), NovaValue::Bool(true)),
                (
                    "c".to_string(),
                    NovaValue::Array(vec![
                        NovaValue::Number(1.0),
                        NovaValue::Array(vec![NovaValue::Number(2.0)]),
                    ])
                ),
                ("d".to_string(), NovaValue::Object(vec![])),
            ])
        );
    }

    #[test]
    fn parses_references_as_values() {
        assert_eq!(
            parse("{ \"email\": @env.EMAIL, \"password\": #auth.password }"),
            NovaValue::Object(vec![
                (
                    "email".to_string(),
                    NovaValue::Ref(Reference {
                        sigil: Sigil::At,
                        segments: vec!["env".to_string(), "EMAIL".to_string()],
                    })
                ),
                (
                    "password".to_string(),
                    NovaValue::Ref(Reference {
                        sigil: Sigil::Hash,
                        segments: vec!["auth".to_string(), "password".to_string()],
                    })
                ),
            ])
        );
    }

    /// Nova bodies are hand-edited and diff-reviewed, so a trailing comma is
    /// allowed here even though `json_parser` still rejects it.
    #[test]
    fn accepts_trailing_commas() {
        assert_eq!(
            parse("{ \"a\": 1, }"),
            NovaValue::Object(vec![("a".to_string(), NovaValue::Number(1.0))])
        );
        assert_eq!(
            parse("[ 1, 2, ]"),
            NovaValue::Array(vec![NovaValue::Number(1.0), NovaValue::Number(2.0)])
        );
    }

    #[test]
    fn rejects_a_comma_with_no_value() {
        assert!(body().parse("[ , ]").is_err());
        assert!(body().parse("{ , }").is_err());
    }

    #[test]
    fn allows_comments_inside_a_body() {
        assert_eq!(
            parse("{\n  // the user\n  \"a\": 1\n}"),
            NovaValue::Object(vec![("a".to_string(), NovaValue::Number(1.0))])
        );
    }

    #[test]
    fn a_quoted_reference_is_just_a_string() {
        assert_eq!(
            parse(r#"{ "a": "@env.EMAIL" }"#),
            NovaValue::Object(vec![("a".to_string(), string("@env.EMAIL"))])
        );
    }

    #[test]
    fn bodies_must_be_objects_or_arrays() {
        assert!(body().parse("\"just a string\"").is_err());
        assert!(body().parse("42").is_err());
    }

    #[test]
    fn line_value_leaves_the_line_ending_alone() {
        assert_eq!(
            line_value().parse("\"john\"\nnext"),
            Ok((string("john"), "\nnext"))
        );
    }

    #[test]
    fn string_array_reads_a_list_of_strings() {
        assert_eq!(
            string_array().parse(r#"[ "email", "user_id" ]"#),
            Ok((vec!["email".to_string(), "user_id".to_string()], ""))
        );
        assert_eq!(string_array().parse("[]"), Ok((Vec::new(), "")));
    }
}
