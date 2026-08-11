pub mod ast;
pub mod lex;
pub mod statement;
pub mod value;

pub use ast::*;
pub use value::NovaValue;

use crate::parser::nova::lex::blank_lines;
use crate::parser::nova::statement::statement;
use crate::parser::parser::{ParseError, Parser};

/// Parses a complete `.nova` document, rejecting any trailing input.
pub fn parse_nova(input: &str) -> Result<Document, ParseError> {
    let statements = statement().ignore_right(blank_lines()).many1();
    let document = blank_lines().ignore_left(statements);

    let (mut statements, remaining) = document.parse(input)?;

    if !remaining.is_empty() {
        return Err(ParseError::new(remaining, "Expected end of input"));
    }

    // Statements record how much input was left when they started, since a
    // parser only ever sees a suffix. Now that the total length is known,
    // convert those to absolute offsets.
    for statement in &mut statements {
        statement.offset = input.len() - statement.offset;
    }

    return Ok(Document { statements });
}

/// Converts a byte offset into a 1-based line and column.
///
/// `ParseError::position` yields a byte offset, which is fine for JSON but of
/// little use to someone editing a line-oriented file.
pub fn line_column(input: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(input.len());
    let before = &input[..offset];
    let line = before.matches('\n').count() + 1;

    let column = match before.rfind('\n') {
        Some(index) => before[index + 1..].chars().count() + 1,
        None => before.chars().count() + 1,
    };

    return (line, column);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_sequence_of_statements() {
        let document = parse_nova("@host\nhttp://localhost:3000\n\nGET /me\n")
            .expect("document should parse");

        assert_eq!(document.statements.len(), 2);
        assert!(matches!(document.statements[0].kind, StatementKind::Host(_)));
        assert!(matches!(document.statements[1].kind, StatementKind::Request(_)));
    }

    #[test]
    fn records_where_each_statement_began() {
        let source = "@host\nhttp://localhost:3000\n\nGET /me\n";
        let document = parse_nova(source).expect("document should parse");

        assert_eq!(document.statements[0].offset, 0);
        assert_eq!(document.statements[1].offset, source.find("GET").unwrap());
    }

    #[test]
    fn tolerates_leading_blank_and_comment_lines() {
        let document = parse_nova("\n// a note\n\n@host\nhttp://x\n")
            .expect("document should parse");

        assert_eq!(document.statements.len(), 1);
    }

    #[test]
    fn rejects_an_empty_document() {
        assert!(parse_nova("").is_err());
        assert!(parse_nova("\n\n").is_err());
    }

    #[test]
    fn rejects_trailing_garbage() {
        assert!(parse_nova("GET /me\n!!!\n").is_err());
    }

    #[test]
    fn line_column_is_one_based() {
        let source = "@host\nhttp://x\n";

        assert_eq!(line_column(source, 0), (1, 1));
        assert_eq!(line_column(source, 6), (2, 1));
        assert_eq!(line_column(source, 9), (2, 4));
    }
}
