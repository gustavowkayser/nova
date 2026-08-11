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
}
