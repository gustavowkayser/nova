use crate::parser::nova::ast::{Reference, Sigil, Template, TemplatePart};
use crate::parser::parser::{ParseError, Parser};

/// Spaces and tabs, never a newline.
///
/// The whole line-oriented grammar rests on this: if horizontal space and
/// vertical space were the same thing, no block could tell where its lines end.
pub fn hspace() -> Parser<()> {
    Parser::<char>::any(" \t".chars()).many().apply_return(())
}

/// One or more spaces or tabs.
pub fn hspace1() -> Parser<()> {
    Parser::<char>::any(" \t".chars()).many1().apply_return(())
}

/// `//` up to, but not including, the newline.
pub fn comment() -> Parser<()> {
    let marker = Parser::<char>::string("//".to_string());
    let text = Parser::<char>::satisfy(|found| found != '\n', "a comment character").many();

    marker.ignore_left(text).apply_return(())
}

/// A line terminator, tolerating CRLF.
pub fn line_break() -> Parser<()> {
    Parser::<char>::opt(Parser::<char>::char('\r'))
        .ignore_left(Parser::<char>::char('\n'))
        .apply_return(())
}

/// Succeeds only at the end of the input, consuming nothing.
pub fn end_of_input() -> Parser<()> {
    Parser::<()>::new(|input| {
        if input.is_empty() {
            return Ok(((), input));
        }

        return Err(ParseError::new(input, "Expected end of line"));
    })
}

/// The end of a line: trailing spaces, an optional comment, then a newline —
/// or the end of the file, so a file need not end with a blank line.
pub fn eol() -> Parser<()> {
    hspace()
        .ignore_left(Parser::<()>::opt(comment()))
        .ignore_left(line_break().or(end_of_input()))
}

/// A line holding nothing but spaces and perhaps a comment.
fn blank() -> Parser<()> {
    hspace()
        .ignore_left(Parser::<()>::opt(comment()))
        .ignore_left(line_break())
}

/// Any run of blank lines. Blank lines carry no meaning anywhere in the
/// grammar; they are formatting only.
///
/// Note this requires an actual `line_break`, so it always consumes input when
/// it matches — which is what keeps `many` from spinning.
pub fn blank_lines() -> Parser<()> {
    blank().many().apply_return(())
}

/// An identifier: a letter or underscore, then letters, digits, underscores
/// and dashes.
pub fn ident() -> Parser<String> {
    let first = Parser::<char>::satisfy(
        |found| found.is_ascii_alphabetic() || found == '_',
        "an identifier",
    );
    let rest = Parser::<char>::satisfy(
        |found| found.is_ascii_alphanumeric() || found == '_' || found == '-',
        "an identifier character",
    )
    .many();

    first.then(rest).map(|(head, tail)| {
        let mut name = String::from(head);
        name.extend(tail);
        return name;
    })
}

/// A header field name. Looser than [`ident`] because HTTP header names may
/// begin with a digit.
pub fn header_name() -> Parser<String> {
    Parser::<char>::satisfy(
        |found| found.is_ascii_alphanumeric() || found == '_' || found == '-',
        "a header name",
    )
    .many1()
    .map(|found: Vec<char>| found.into_iter().collect())
}

/// `@name`, `@a.b.c`, `#auth.email`.
pub fn reference() -> Parser<Reference> {
    let sigil = Parser::<char>::char('@')
        .apply_return(Sigil::At)
        .or(Parser::<char>::char('#').apply_return(Sigil::Hash));
    let segments = ident().sep_by1(Parser::<char>::char('.'));

    sigil
        .then(segments)
        .map(|(sigil, segments)| Reference { sigil, segments })
        .label("a reference")
}

/// Line text mixing literal runs with references.
pub fn template() -> Parser<Template> {
    let part = reference()
        .map(TemplatePart::Ref)
        .or(literal_run().map(TemplatePart::Literal));

    part.many1()
        .map(|parts| Template { parts: trim_trailing_space(parts) })
        .label("line content")
}

/// Literal text up to a reference, a comment, or the end of the line.
///
/// The comment rule is the interesting part. A `//` only starts a comment when
/// it follows a space or a tab, so the `//` in `http://localhost:3000` is
/// preceded by `:` and stays in the URL, while `GET /me // note` splits where
/// it should. Without this the language could not have both `//` comments and
/// bare URLs.
fn literal_run() -> Parser<String> {
    Parser::<String>::new(|input| {
        let bytes = input.as_bytes();
        let mut end = 0;

        for (offset, character) in input.char_indices() {
            if matches!(character, '\n' | '\r' | '@' | '#') {
                break;
            }

            let after_space = offset == 0
                || bytes[offset - 1] == b' '
                || bytes[offset - 1] == b'\t';

            if after_space && input[offset..].starts_with("//") {
                break;
            }

            end = offset + character.len_utf8();
        }

        if end == 0 {
            return Err(ParseError::new(input, "Expected line content"));
        }

        return Ok((input[..end].to_string(), &input[end..]));
    })
}

/// Drops the trailing spaces a line picks up before its newline or comment,
/// so `GET /me   ` and `GET /me // note` both yield the path `/me`.
fn trim_trailing_space(mut parts: Vec<TemplatePart>) -> Vec<TemplatePart> {
    let now_empty = match parts.last_mut() {
        Some(TemplatePart::Literal(text)) => {
            let kept = text.trim_end_matches([' ', '\t']).len();
            text.truncate(kept);
            kept == 0
        }
        _ => false,
    };

    if now_empty {
        parts.pop();
    }

    return parts;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hspace_stops_at_a_newline() {
        assert_eq!(hspace().parse("  \t x"), Ok(((), "x")));
        assert_eq!(hspace().parse("  \nx"), Ok(((), "\nx")));
        assert_eq!(hspace().parse("x"), Ok(((), "x")));
    }

    #[test]
    fn hspace1_requires_at_least_one_space() {
        assert_eq!(hspace1().parse(" x"), Ok(((), "x")));
        assert!(hspace1().parse("x").is_err());
    }

    #[test]
    fn comment_runs_to_the_end_of_the_line() {
        assert_eq!(comment().parse("// note\nrest"), Ok(((), "\nrest")));
        assert_eq!(comment().parse("//"), Ok(((), "")));
        assert!(comment().parse("/ not a comment").is_err());
    }

    #[test]
    fn eol_accepts_a_newline_or_end_of_input() {
        assert_eq!(eol().parse("\nrest"), Ok(((), "rest")));
        assert_eq!(eol().parse("\r\nrest"), Ok(((), "rest")));
        assert_eq!(eol().parse("   \nrest"), Ok(((), "rest")));
        assert_eq!(eol().parse("  // trailing\nrest"), Ok(((), "rest")));
        assert_eq!(eol().parse(""), Ok(((), "")));
        assert!(eol().parse("x\n").is_err());
    }

    #[test]
    fn blank_lines_eat_empty_and_comment_only_lines() {
        assert_eq!(blank_lines().parse("\n\n  \n// c\nrest"), Ok(((), "rest")));
        assert_eq!(blank_lines().parse("rest"), Ok(((), "rest")));
    }

    #[test]
    fn ident_accepts_letters_digits_underscores_and_dashes() {
        assert_eq!(ident().parse("accessToken "), Ok(("accessToken".to_string(), " ")));
        assert_eq!(ident().parse("user_id."), Ok(("user_id".to_string(), ".")));
        assert_eq!(ident().parse("get-user)"), Ok(("get-user".to_string(), ")")));
        assert_eq!(ident().parse("_private"), Ok(("_private".to_string(), "")));
    }

    #[test]
    fn ident_rejects_a_leading_digit_or_dash() {
        assert!(ident().parse("1abc").is_err());
        assert!(ident().parse("-abc").is_err());
        assert!(ident().parse("").is_err());
    }

    #[test]
    fn header_name_allows_a_leading_digit() {
        assert_eq!(header_name().parse("Content-Type:"), Ok(("Content-Type".to_string(), ":")));
        assert_eq!(header_name().parse("X-2-Header:"), Ok(("X-2-Header".to_string(), ":")));
        assert!(header_name().parse(":").is_err());
    }

    fn at(segments: &[&str]) -> Reference {
        Reference {
            sigil: Sigil::At,
            segments: segments.iter().map(|part| part.to_string()).collect(),
        }
    }

    fn literal(text: &str) -> TemplatePart {
        TemplatePart::Literal(text.to_string())
    }

    #[test]
    fn reference_reads_a_sigil_and_dotted_segments() {
        assert_eq!(reference().parse("@accessToken"), Ok((at(&["accessToken"]), "")));
        assert_eq!(reference().parse("@env.EMAIL "), Ok((at(&["env", "EMAIL"]), " ")));
        assert_eq!(
            reference().parse("@login.response.body.accessToken"),
            Ok((at(&["login", "response", "body", "accessToken"]), ""))
        );
        assert_eq!(
            reference().parse("#auth.email"),
            Ok((
                Reference {
                    sigil: Sigil::Hash,
                    segments: vec!["auth".to_string(), "email".to_string()],
                },
                ""
            ))
        );
        assert!(reference().parse("@").is_err());
        assert!(reference().parse("login").is_err());
    }

    #[test]
    fn template_interleaves_literals_and_references() {
        assert_eq!(
            template().parse("Bearer @accessToken\n"),
            Ok((
                Template { parts: vec![literal("Bearer "), TemplatePart::Ref(at(&["accessToken"]))] },
                "\n"
            ))
        );
        assert_eq!(
            template().parse("/users/@userId/posts\n"),
            Ok((
                Template {
                    parts: vec![
                        literal("/users/"),
                        TemplatePart::Ref(at(&["userId"])),
                        literal("/posts"),
                    ]
                },
                "\n"
            ))
        );
    }

    /// The rule that makes `//` comments possible at all: without it, every
    /// `@host` line in the language parses as `http:` plus a comment.
    #[test]
    fn a_double_slash_is_only_a_comment_after_whitespace() {
        assert_eq!(
            template().parse("http://localhost:3000\n"),
            Ok((Template { parts: vec![literal("http://localhost:3000")] }, "\n"))
        );
        assert_eq!(
            template().parse("/a//b\n"),
            Ok((Template { parts: vec![literal("/a//b")] }, "\n"))
        );
        assert_eq!(
            template().parse("/me // check this\n"),
            Ok((Template { parts: vec![literal("/me")] }, "// check this\n"))
        );
    }

    #[test]
    fn template_trims_trailing_horizontal_space() {
        assert_eq!(
            template().parse("/me   \n"),
            Ok((Template { parts: vec![literal("/me")] }, "\n"))
        );
    }

    #[test]
    fn template_stops_at_the_end_of_the_line() {
        assert_eq!(
            template().parse("one\ntwo"),
            Ok((Template { parts: vec![literal("one")] }, "\ntwo"))
        );
        assert!(template().parse("\n").is_err());
    }
}
