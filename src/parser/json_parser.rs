use crate::parser::parser::{ParseError, Parser};

/// The JSON data model (RFC 8259).
///
/// Objects keep their members in source order, so `Vec<(String, JsonValue)>`
/// is used instead of a map. Use [`JsonValue::get`] to look a member up.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// Looks up a member of an object. Returns `None` for any other variant.
    pub fn get(&self, key: &str) -> Option<&JsonValue> {
        match self {
            JsonValue::Object(members) => members
                .iter()
                .find(|(name, _value)| name == key)
                .map(|(_name, value)| value),
            _ => None,
        }
    }
}

/// Parses a complete JSON document, rejecting any trailing input.
pub fn parse_json(input: &str) -> Result<JsonValue, ParseError> {
    let document = whitespace().ignore_left(value());
    let (parsed, remaining) = document.parse(input)?;

    if remaining.is_empty() {
        return Ok(parsed);
    }

    return Err(ParseError::new(remaining, "Expected end of input"));
}

/// A single JSON value, plus any whitespace that follows it.
///
/// This is the recursion knot of the grammar: the parser tree is only built
/// once parsing actually reaches this point, so `array`/`object` can refer
/// back to `value` without building an infinitely deep parser up front.
pub fn value() -> Parser<JsonValue> {
    return Parser::<JsonValue>::new(|input| value_impl().parse(input));
}

fn value_impl() -> Parser<JsonValue> {
    let alternatives = vec![
        null(),
        boolean(),
        string_value(),
        number_value(),
        array(),
        object(),
    ];

    return token(Parser::<JsonValue>::choice(alternatives).label("a JSON value"));
}

fn null() -> Parser<JsonValue> {
    return keyword("null").apply_return(JsonValue::Null);
}

fn boolean() -> Parser<JsonValue> {
    let parse_true = keyword("true").apply_return(JsonValue::Bool(true));
    let parse_false = keyword("false").apply_return(JsonValue::Bool(false));

    return parse_true.or(parse_false);
}

fn string_value() -> Parser<JsonValue> {
    return string_literal().map(JsonValue::String);
}

fn number_value() -> Parser<JsonValue> {
    return number().map(JsonValue::Number);
}

fn array() -> Parser<JsonValue> {
    let elements = value().sep_by(token(Parser::<char>::char(',')));

    return token(Parser::<char>::char('['))
        .ignore_left(elements)
        .ignore_right(token(Parser::<char>::char(']')))
        .map(JsonValue::Array);
}

fn object() -> Parser<JsonValue> {
    let members = member().sep_by(token(Parser::<char>::char(',')));

    return token(Parser::<char>::char('{'))
        .ignore_left(members)
        .ignore_right(token(Parser::<char>::char('}')))
        .map(JsonValue::Object);
}

fn member() -> Parser<(String, JsonValue)> {
    let key = token(string_literal());

    return key
        .ignore_right(token(Parser::<char>::char(':')))
        .then(value());
}

// --- Strings ---------------------------------------------------------------

/// A quoted JSON string, with escape sequences resolved.
fn string_literal() -> Parser<String> {
    let quote = Parser::<char>::char('"');

    return quote
        .clone()
        .ignore_left(string_char().many())
        .ignore_right(quote)
        .map(|chars: Vec<char>| chars.into_iter().collect());
}

fn string_char() -> Parser<char> {
    let escaped = Parser::<char>::char('\\').ignore_left(escape_sequence());
    let unescaped = Parser::<char>::satisfy(
        |found| found != '"' && found != '\\' && !found.is_control(),
        "a string character",
    );

    return escaped.or(unescaped);
}

fn escape_sequence() -> Parser<char> {
    let simple = vec![
        Parser::<char>::char('"').apply_return('"'),
        Parser::<char>::char('\\').apply_return('\\'),
        Parser::<char>::char('/').apply_return('/'),
        Parser::<char>::char('b').apply_return('\u{0008}'),
        Parser::<char>::char('f').apply_return('\u{000C}'),
        Parser::<char>::char('n').apply_return('\n'),
        Parser::<char>::char('r').apply_return('\r'),
        Parser::<char>::char('t').apply_return('\t'),
        unicode_escape(),
    ];

    return Parser::<char>::choice(simple);
}

/// `uXXXX`, joining a surrogate pair into a single `char` when one is present.
fn unicode_escape() -> Parser<char> {
    let leading = Parser::<char>::char('u').ignore_left(hex4());
    let trailing = Parser::<char>::char('\\')
        .ignore_left(Parser::<char>::char('u'))
        .ignore_left(hex4());

    return Parser::<char>::new(move |input| {
        let (high, after_high) = leading.parse(input)?;

        if (0xD800..0xDC00).contains(&high) {
            let (low, after_low) = trailing.parse(after_high)?;

            if !(0xDC00..0xE000).contains(&low) {
                return Err(ParseError::new(
                    after_high,
                    format!("Expected a low surrogate, found \\u{low:04X}"),
                ));
            }

            let combined = 0x10000 + ((high - 0xD800) << 10) + (low - 0xDC00);

            return to_char(combined, after_low).map(|found| (found, after_low));
        }

        if (0xDC00..0xE000).contains(&high) {
            return Err(ParseError::new(
                after_high,
                format!("Unpaired low surrogate \\u{high:04X}"),
            ));
        }

        return to_char(high, after_high).map(|found| (found, after_high));
    });
}

fn to_char(code_point: u32, remaining: &str) -> Result<char, ParseError> {
    return char::from_u32(code_point).ok_or_else(|| {
        ParseError::new(remaining, format!("Invalid code point U+{code_point:04X}"))
    });
}

fn hex4() -> Parser<u32> {
    let digits = Parser::<char>::sequence(vec![hex_digit(); 4]);

    return digits.map(|found| {
        found
            .iter()
            .fold(0u32, |accumulator, digit| {
                accumulator * 16 + digit.to_digit(16).unwrap()
            })
    });
}

fn hex_digit() -> Parser<char> {
    return Parser::<char>::any("0123456789abcdefABCDEF".chars());
}

// --- Numbers ---------------------------------------------------------------

fn number() -> Parser<f64> {
    let text = number_text();

    return Parser::<f64>::new(move |input| {
        let (found, remaining) = text.parse(input)?;

        return found
            .parse::<f64>()
            .map(|number| (number, remaining))
            .map_err(|error| ParseError::new(input, format!("Invalid number {found:?}: {error}")));
    });
}

/// The literal text of a JSON number, validated against the grammar
/// (an optional sign, no leading zeros, optional fraction and exponent).
fn number_text() -> Parser<String> {
    let digit = Parser::<char>::any("0123456789".chars());
    let digits = digit.clone().many1().map(collect_string);

    let sign = Parser::<char>::opt(Parser::<char>::char('-')).map(optional_char);

    let zero = Parser::<char>::char('0').map(String::from);
    let non_zero = Parser::<char>::any("123456789".chars())
        .then(digit.many())
        .map(|(first, rest)| {
            let mut text = String::from(first);
            text.extend(rest);
            return text;
        });
    let integer = non_zero.or(zero);

    let fraction = Parser::<char>::opt(
        Parser::<char>::char('.').ignore_left(digits.clone()),
    )
    .map(|found| match found {
        Some(text) => format!(".{text}"),
        None => String::new(),
    });

    let exponent_sign = Parser::<char>::opt(Parser::<char>::any("+-".chars())).map(optional_char);
    let exponent = Parser::<char>::opt(
        Parser::<char>::any("eE".chars())
            .then(exponent_sign)
            .then(digits)
            .map(|((marker, sign), digits)| format!("{marker}{sign}{digits}")),
    )
    .map(|found| found.unwrap_or_default());

    return sign
        .then(integer)
        .then(fraction)
        .then(exponent)
        .map(|(((sign, integer), fraction), exponent)| {
            format!("{sign}{integer}{fraction}{exponent}")
        });
}

fn collect_string(chars: Vec<char>) -> String {
    return chars.into_iter().collect();
}

fn optional_char(found: Option<char>) -> String {
    return found.map(String::from).unwrap_or_default();
}

// --- Lexical helpers -------------------------------------------------------

/// A literal word that must not run into a following identifier character,
/// so `nullable` is not accepted as `null`.
fn keyword(expected: &'static str) -> Parser<String> {
    let word = Parser::<char>::string(expected.to_string());

    return Parser::<String>::new(move |input| {
        let (found, remaining) = word.parse(input)?;

        match remaining.chars().next() {
            Some(next) if next.is_alphanumeric() || next == '_' => Err(ParseError::new(
                remaining,
                format!("Expected {expected:?}, found {expected}{next}..."),
            )),
            _ => Ok((found, remaining)),
        }
    });
}

/// Runs `parser`, then discards any trailing whitespace.
fn token<T>(parser: Parser<T>) -> Parser<T>
where
    T: 'static,
{
    return parser.ignore_right(whitespace());
}

fn whitespace() -> Parser<Vec<char>> {
    return Parser::<char>::any(" \t\r\n".chars()).many();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn error_for(input: &str) -> (String, usize) {
        let error = parse_json(input).expect_err("input should not parse");
        let position = error.position(input);

        return (error.message, position);
    }

    #[test]
    fn reports_the_failure_rather_than_a_generic_message() {
        assert_eq!(
            error_for(""),
            ("Expected a JSON value, found EOF".to_string(), 0)
        );
        assert_eq!(
            error_for("[1, 2,]"),
            ("Expected ']', found ','".to_string(), 5)
        );
        assert_eq!(
            error_for("{1: 2}"),
            ("Expected '}', found '1'".to_string(), 1)
        );
        assert_eq!(
            error_for("nullable"),
            ("Expected \"null\", found nulla...".to_string(), 4)
        );
        assert_eq!(
            error_for("[1] [2]"),
            ("Expected end of input".to_string(), 4)
        );
    }

    #[test]
    fn parses_literals() {
        assert_eq!(parse_json("null"), Ok(JsonValue::Null));
        assert_eq!(parse_json("true"), Ok(JsonValue::Bool(true)));
        assert_eq!(parse_json("false"), Ok(JsonValue::Bool(false)));
    }

    #[test]
    fn rejects_literal_prefixes() {
        assert!(parse_json("nullable").is_err());
        assert!(parse_json("truthy").is_err());
    }

    #[test]
    fn parses_numbers() {
        assert_eq!(parse_json("0"), Ok(JsonValue::Number(0.0)));
        assert_eq!(parse_json("-42"), Ok(JsonValue::Number(-42.0)));
        assert_eq!(parse_json("3.5"), Ok(JsonValue::Number(3.5)));
        assert_eq!(parse_json("1e3"), Ok(JsonValue::Number(1000.0)));
        assert_eq!(parse_json("-1.5E-2"), Ok(JsonValue::Number(-0.015)));
    }

    #[test]
    fn rejects_malformed_numbers() {
        assert!(parse_json("01").is_err());
        assert!(parse_json("1.").is_err());
        assert!(parse_json(".5").is_err());
        assert!(parse_json("1e").is_err());
        assert!(parse_json("+1").is_err());
    }

    #[test]
    fn parses_strings() {
        assert_eq!(
            parse_json(r#""hello""#),
            Ok(JsonValue::String("hello".to_string()))
        );
        assert_eq!(parse_json(r#""""#), Ok(JsonValue::String(String::new())));
        assert_eq!(
            parse_json(r#""a\"b\\c\/d\n\t""#),
            Ok(JsonValue::String("a\"b\\c/d\n\t".to_string()))
        );
    }

    #[test]
    fn parses_unicode_escapes() {
        assert_eq!(
            parse_json(r#""é""#),
            Ok(JsonValue::String("é".to_string()))
        );
        assert_eq!(
            parse_json(r#""😀""#),
            Ok(JsonValue::String("😀".to_string()))
        );
        assert!(parse_json(r#""\uD83D""#).is_err());
        assert!(parse_json(r#""\uDE00""#).is_err());
    }

    #[test]
    fn rejects_unterminated_and_control_characters() {
        assert!(parse_json(r#""abc"#).is_err());
        assert!(parse_json("\"a\nb\"").is_err());
    }

    #[test]
    fn parses_arrays() {
        assert_eq!(parse_json("[]"), Ok(JsonValue::Array(vec![])));
        assert_eq!(parse_json("[ ]"), Ok(JsonValue::Array(vec![])));
        assert_eq!(
            parse_json("[1, 2, 3]"),
            Ok(JsonValue::Array(vec![
                JsonValue::Number(1.0),
                JsonValue::Number(2.0),
                JsonValue::Number(3.0),
            ]))
        );
        assert_eq!(
            parse_json("[[1], []]"),
            Ok(JsonValue::Array(vec![
                JsonValue::Array(vec![JsonValue::Number(1.0)]),
                JsonValue::Array(vec![]),
            ]))
        );
    }

    #[test]
    fn parses_objects() {
        assert_eq!(parse_json("{}"), Ok(JsonValue::Object(vec![])));
        assert_eq!(
            parse_json(r#"{ "a" : 1 , "b" : null }"#),
            Ok(JsonValue::Object(vec![
                ("a".to_string(), JsonValue::Number(1.0)),
                ("b".to_string(), JsonValue::Null),
            ]))
        );
    }

    #[test]
    fn rejects_trailing_commas_and_garbage() {
        assert!(parse_json("[1, 2,]").is_err());
        assert!(parse_json(r#"{"a": 1,}"#).is_err());
        assert!(parse_json("{1: 2}").is_err());
        assert!(parse_json("[1] [2]").is_err());
        assert!(parse_json("").is_err());
    }

    #[test]
    fn parses_a_nested_document() {
        let document = r#"
        {
            "name": "nova",
            "version": 0.1,
            "tags": ["parser", "json"],
            "meta": { "stable": false, "downloads": null }
        }
        "#;

        let parsed = parse_json(document).expect("document should parse");

        assert_eq!(parsed.get("name"), Some(&JsonValue::String("nova".to_string())));
        assert_eq!(parsed.get("version"), Some(&JsonValue::Number(0.1)));
        assert_eq!(
            parsed.get("tags"),
            Some(&JsonValue::Array(vec![
                JsonValue::String("parser".to_string()),
                JsonValue::String("json".to_string()),
            ]))
        );
        assert_eq!(
            parsed.get("meta").and_then(|meta| meta.get("stable")),
            Some(&JsonValue::Bool(false))
        );
        assert_eq!(parsed.get("missing"), None);
    }
}
