use crate::parser::nova::ast::*;
use crate::parser::nova::lex::{blank_lines, eol, header_name, hspace, ident, template};
use crate::parser::parser::{ParseError, Parser};

/// One statement, tagged with where it started.
///
/// `offset` is recorded as *bytes remaining* here, because a parser only ever
/// sees a suffix of the document and cannot know its own absolute position.
/// [`super::parse_nova`] converts these to real offsets once it knows the
/// total length.
pub fn statement() -> Parser<Statement> {
    let kind = Parser::<StatementKind>::choice(vec![host(), headers()])
        .label("a Nova statement");

    return Parser::<Statement>::new(move |input| {
        let remaining_at_start = input.len();
        let (kind, remaining) = kind.parse(input)?;

        return Ok((Statement { kind, offset: remaining_at_start }, remaining));
    });
}

fn host() -> Parser<StatementKind> {
    return marker("host")
        .ignore_left(eol())
        .ignore_left(blank_lines())
        .ignore_left(template().label("a host URL"))
        .ignore_right(eol())
        .map(StatementKind::Host);
}

fn headers() -> Parser<StatementKind> {
    let lines = header_line().ignore_right(blank_lines()).many1();

    return marker("header")
        .ignore_left(eol())
        .ignore_left(blank_lines())
        .ignore_left(lines)
        .map(StatementKind::Headers);
}

fn header_line() -> Parser<Header> {
    return header_name()
        .ignore_right(Parser::<char>::char(':'))
        .ignore_right(hspace())
        .then(template().label("a header value"))
        .ignore_right(eol())
        .map(|(name, value)| Header { name, value });
}

/// `@word`, where the word must match exactly — so `@hostname` is not read as
/// `@host` followed by junk.
fn marker(word: &'static str) -> Parser<String> {
    return Parser::<char>::char('@').ignore_left(exact_ident(word));
}

fn exact_ident(word: &'static str) -> Parser<String> {
    return Parser::<String>::new(move |input| {
        let (found, remaining) = ident().parse(input)?;

        if found == word {
            return Ok((found, remaining));
        }

        return Err(ParseError::new(input, format!("Expected {word:?}, found {found:?}")));
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::nova::ast::{Sigil, TemplatePart};

    fn literal(text: &str) -> Template {
        Template { parts: vec![TemplatePart::Literal(text.to_string())] }
    }

    fn at(segments: &[&str]) -> Reference {
        Reference {
            sigil: Sigil::At,
            segments: segments.iter().map(|part| part.to_string()).collect(),
        }
    }

    fn kind(input: &str) -> StatementKind {
        let (parsed, _remaining) = statement().parse(input).expect("statement should parse");
        return parsed.kind;
    }

    #[test]
    fn parses_a_host_block() {
        assert_eq!(
            kind("@host\nhttp://localhost:3000\n"),
            StatementKind::Host(literal("http://localhost:3000"))
        );
    }

    #[test]
    fn a_host_may_be_a_reference() {
        assert_eq!(
            kind("@host\n@env.BASE_URL\n"),
            StatementKind::Host(Template { parts: vec![TemplatePart::Ref(at(&["env", "BASE_URL"]))] })
        );
    }

    #[test]
    fn parses_a_header_block() {
        assert_eq!(
            kind("@header\nContent-Type: application/json\n"),
            StatementKind::Headers(vec![Header {
                name: "Content-Type".to_string(),
                value: literal("application/json"),
            }])
        );
    }

    #[test]
    fn a_header_value_may_interpolate() {
        assert_eq!(
            kind("@header\nAuthorization: Bearer @accessToken\n"),
            StatementKind::Headers(vec![Header {
                name: "Authorization".to_string(),
                value: Template {
                    parts: vec![
                        TemplatePart::Literal("Bearer ".to_string()),
                        TemplatePart::Ref(at(&["accessToken"])),
                    ],
                },
            }])
        );
    }

    /// Decision 4 in the spec: a blank line is formatting, not a terminator.
    /// The block ends at the first line that cannot be a header.
    #[test]
    fn a_header_block_survives_blank_lines_and_ends_on_a_foreign_line() {
        let (parsed, remaining) = statement()
            .parse("@header\nA: 1\n\nB: 2\n\n@login\nPOST /x\n")
            .expect("statement should parse");

        assert_eq!(
            parsed.kind,
            StatementKind::Headers(vec![
                Header { name: "A".to_string(), value: literal("1") },
                Header { name: "B".to_string(), value: literal("2") },
            ])
        );
        assert_eq!(remaining, "@login\nPOST /x\n");
    }

    #[test]
    fn a_header_block_ends_before_a_request_line() {
        let (_parsed, remaining) = statement()
            .parse("@header\nA: 1\nGET /me\n")
            .expect("statement should parse");

        assert_eq!(remaining, "GET /me\n");
    }
}
