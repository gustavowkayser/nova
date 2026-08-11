# Nova Language Parser Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parse `.nova` source files into a flat, ordered AST, per `docs/superpowers/specs/2026-08-11-nova-parser-design.md`.

**Architecture:** A new `src/parser/nova/` module built on the existing combinator library in `src/parser/parser.rs`, which is not modified. Two whitespace regimes: the statement grammar is line-oriented (`hspace` never matches `\n`), while request bodies are a free-form island where newlines are insignificant. `json_parser.rs` changes only in the visibility of three functions.

**Tech Stack:** Rust 1.97, edition 2024. No new dependencies. Tests are `#[cfg(test)] mod tests` blocks inside each file, matching `json_parser.rs`.

---

## Background for the implementer

You will not have seen this codebase. Read these two files before starting:

- `src/parser/parser.rs` — the combinator library. Everything is built from it.
- `src/parser/json_parser.rs` — a worked example of the style to follow.

**Combinator cheat sheet** (all from `parser.rs`):

| Call | Meaning |
| --- | --- |
| `Parser::<char>::char('x')` | one literal character |
| `Parser::<char>::string("ab".to_string())` | a literal string |
| `Parser::<char>::satisfy(pred, "desc")` | one char matching a predicate |
| `Parser::<char>::any("abc".chars())` | one char from a set |
| `Parser::<X>::opt(p)` | `Parser<Option<A>>`; backtracks fully on failure |
| `Parser::<X>::choice(vec![..])` | first success wins; keeps the *deepest* error |
| `Parser::<X>::returnp(v)` | always succeeds, consumes nothing |
| `p.many()` / `p.many1()` | zero-or-more / one-or-more |
| `p.then(q)` | `Parser<(T, U)>` |
| `p.ignore_left(q)` | run both, keep `q`'s value |
| `p.ignore_right(q)` | run both, keep `p`'s value |
| `p.or(q)` | try `p`, else `q` |
| `p.map(f)` / `p.apply_return(v)` | transform / replace the value |
| `p.sep_by(s)` / `p.sep_by1(s)` | separated lists |
| `p.label("a thing")` | rename the expected-item in errors |

**Three traps specific to this library:**

1. `Parser::<X>::opt` and `choice` and `or` are associated functions on a generic
   impl. The turbofish type is irrelevant — `Parser::<char>::opt(string_parser)`
   is fine and is used throughout this plan.
2. `many()` recurses forever if its inner parser can succeed without consuming
   input. Every parser passed to `many`/`many1` in this plan consumes at least
   one byte on success. Preserve that property.
3. Recursive grammars must be tied with a lazy knot, or building the parser
   tree never terminates. `json_parser::value` shows the pattern:
   `Parser::<T>::new(|input| real_impl().parse(input))`. Used here for
   `value_core` and `request`.

**Running tests:** this is a binary crate, so everything runs under `cargo test`.
Filter by module path, e.g. `cargo test parser::nova::lex`.

**Baseline:** `cargo test` currently passes 12 tests. It must still pass 12
`json_parser` tests at the end — that suite is not to be modified.

---

## File structure

| File | Responsibility |
| --- | --- |
| `src/parser/nova/ast.rs` | Type definitions only. No parsing logic. |
| `src/parser/nova/lex.rs` | Line-aware lexical layer: whitespace, comments, line ends, identifiers, references, templates. |
| `src/parser/nova/value.rs` | `NovaValue` and the free-form body parser. |
| `src/parser/nova/statement.rs` | The statement grammar. |
| `src/parser/nova/mod.rs` | `parse_nova`, `line_column`, re-exports. |
| `src/parser/json_parser.rs` | **Modified:** three functions become `pub(crate)`. |
| `src/parser.rs` | **Modified:** add `pub mod nova;`. |
| `src/parser/nova_parser.rs` | **Deleted** (empty placeholder, untracked). |

**Two refinements to the spec's table, both deliberate:**

- The spec listed `lex.rs` as depending only on `parser.rs`. It also needs
  `ast.rs`, because `reference()` and `template()` build `Reference` and
  `Template` values. Still no cycle.
- The spec named `string_literal` and `number` as the functions to expose from
  `json_parser`. `keyword` is needed too, for `null`/`true`/`false` in bodies
  and for type names. Duplicating it would put the "does this literal run into
  a longer identifier?" rule in two places.

`ast.rs` and `value.rs` refer to each other (`Request` holds a `NovaValue`;
`NovaValue::Ref` holds a `Reference`). Rust permits mutually-referencing
modules — this is not a problem and needs no workaround.

---

## Task 1: Scaffold the module and define the AST

**Files:**
- Create: `src/parser/nova/ast.rs`
- Create: `src/parser/nova/mod.rs`
- Modify: `src/parser.rs`
- Delete: `src/parser/nova_parser.rs`

No tests in this task — it is pure type declarations, and there is no behaviour
to assert yet. Task 2 begins the TDD cycle.

- [ ] **Step 1: Delete the empty placeholder**

```bash
rm src/parser/nova_parser.rs
```

- [ ] **Step 2: Create `src/parser/nova/ast.rs`**

```rust
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
```

- [ ] **Step 3: Create `src/parser/nova/mod.rs`**

The real `parse_nova` arrives in Task 10; this is just the module wiring.

```rust
pub mod ast;
pub mod lex;
pub mod statement;
pub mod value;

pub use ast::*;
pub use value::NovaValue;
```

- [ ] **Step 4: Create empty `lex.rs`, `value.rs`, `statement.rs` so the crate compiles**

```bash
touch src/parser/nova/lex.rs src/parser/nova/value.rs src/parser/nova/statement.rs
```

`ast.rs` imports `value::NovaValue`, so add just that type to `value.rs` for now
— Task 5 fills in the rest of the file:

```rust
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
```

- [ ] **Step 5: Register the module in `src/parser.rs`**

Replace the whole file with:

```rust
pub mod parser;
pub mod json_parser;
pub mod nova;
```

- [ ] **Step 6: Verify it compiles**

Run: `cargo build`
Expected: success. Warnings about unused code are fine at this stage.

- [ ] **Step 7: Commit**

```bash
git add src/parser.rs src/parser/nova/
git rm --cached src/parser/nova_parser.rs 2>/dev/null || true
git commit -m "feat: Scaffold the nova parser module and its AST"
```

---

## Task 2: Lexical layer — whitespace, comments, line endings

**Files:**
- Modify: `src/parser/nova/lex.rs`

This is where the two whitespace regimes are separated. `hspace` must never
match a newline — that single property is what makes the statement grammar
line-oriented.

- [ ] **Step 1: Write the failing tests**

Append to `src/parser/nova/lex.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::lex`
Expected: compile error — `hspace`, `hspace1`, `comment`, `eol`, `blank_lines` not found.

- [ ] **Step 3: Write the implementation**

Insert *above* the `mod tests` block in `src/parser/nova/lex.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::lex`
Expected: 5 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/lex.rs
git commit -m "feat: Add line-aware whitespace and comment parsers"
```

---

## Task 3: Lexical layer — identifiers

**Files:**
- Modify: `src/parser/nova/lex.rs`

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` block in `src/parser/nova/lex.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::lex`
Expected: compile error — `ident`, `header_name` not found.

- [ ] **Step 3: Write the implementation**

Add to `src/parser/nova/lex.rs`, above `mod tests`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::lex`
Expected: 8 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/lex.rs
git commit -m "feat: Add identifier and header-name parsers"
```

---

## Task 4: Lexical layer — references and templates

**Files:**
- Modify: `src/parser/nova/lex.rs`

This task contains the `http://` rule from the spec. `literal_run` is the only
production that greedily consumes arbitrary line text, so it is the only one
that has to decide whether a `//` is a comment or part of a URL.

- [ ] **Step 1: Write the failing tests**

Add these tests inside the existing `mod tests` block in `src/parser/nova/lex.rs`:

```rust
    use crate::parser::nova::ast::{Reference, Sigil, Template, TemplatePart};

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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::lex`
Expected: compile error — `reference`, `template` not found.

- [ ] **Step 3: Write the implementation**

Add to `src/parser/nova/lex.rs`, above `mod tests`. Also add the import
`use crate::parser::nova::ast::{Reference, Sigil, Template, TemplatePart};` to
the top of the file.

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::lex`
Expected: 13 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/lex.rs
git commit -m "feat: Add reference and template parsers with the http:// comment rule"
```

---

## Task 5: Request bodies

**Files:**
- Modify: `src/parser/json_parser.rs` (visibility only)
- Modify: `src/parser/nova/value.rs`

- [ ] **Step 1: Expose the JSON primitives**

In `src/parser/json_parser.rs`, change exactly three signatures. Do not touch
anything else in the file — its 12 tests must continue to pass unmodified.

Line ~112: `fn string_literal() -> Parser<String> {` becomes:

```rust
pub(crate) fn string_literal() -> Parser<String> {
```

Line ~208: `fn number() -> Parser<f64> {` becomes:

```rust
pub(crate) fn number() -> Parser<f64> {
```

Line ~277: `fn keyword(expected: &'static str) -> Parser<String> {` becomes:

```rust
pub(crate) fn keyword(expected: &'static str) -> Parser<String> {
```

- [ ] **Step 2: Write the failing tests**

Append to `src/parser/nova/value.rs`:

```rust
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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test parser::nova::value`
Expected: compile error — `body`, `line_value`, `string_array` not found.

- [ ] **Step 4: Write the implementation**

Replace the whole of `src/parser/nova/value.rs` with:

```rust
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
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test parser::nova::value`
Expected: 10 passed.

- [ ] **Step 6: Confirm the JSON suite is undisturbed**

Run: `cargo test parser::json_parser`
Expected: 12 passed. If any fail, the visibility change touched behaviour — revert and redo Step 1.

- [ ] **Step 7: Commit**

```bash
git add src/parser/json_parser.rs src/parser/nova/value.rs
git commit -m "feat: Add the Nova body parser with references and trailing commas"
```

---

## Task 6: Statements — host and header blocks

**Files:**
- Modify: `src/parser/nova/statement.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/parser/nova/statement.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::statement`
Expected: compile error — `statement` not found.

- [ ] **Step 3: Write the implementation**

Insert above `mod tests` in `src/parser/nova/statement.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::statement`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/statement.rs
git commit -m "feat: Parse @host and @header blocks"
```

---

## Task 7: Statements — commands and variable assignment

**Files:**
- Modify: `src/parser/nova/statement.rs`

- [ ] **Step 1: Write the failing tests**

Add inside the existing `mod tests` block in `src/parser/nova/statement.rs`:

```rust
    #[test]
    fn parses_a_command_declaration() {
        assert_eq!(
            kind("#command auth email password\n"),
            StatementKind::Command(Command {
                name: "auth".to_string(),
                parameters: vec!["email".to_string(), "password".to_string()],
            })
        );
    }

    #[test]
    fn a_command_may_take_no_parameters() {
        assert_eq!(
            kind("#command smoke\n"),
            StatementKind::Command(Command { name: "smoke".to_string(), parameters: vec![] })
        );
    }

    #[test]
    fn parses_a_variable_assignment() {
        assert_eq!(
            kind("@accessToken = @login.response.body.accessToken\n"),
            StatementKind::Assign(Assign {
                name: "accessToken".to_string(),
                value: at(&["login", "response", "body", "accessToken"]),
            })
        );
    }

    #[test]
    fn an_assignment_tolerates_tight_spacing() {
        assert_eq!(
            kind("@token=@env.TOKEN\n"),
            StatementKind::Assign(Assign {
                name: "token".to_string(),
                value: at(&["env", "TOKEN"]),
            })
        );
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::statement`
Expected: 4 failures — `statement should parse` panics, because `choice` does not yet include these forms.

- [ ] **Step 3: Write the implementation**

Add to `src/parser/nova/statement.rs` above `mod tests`, and extend the import
line to include `hspace1` and `reference`:

```rust
use crate::parser::nova::lex::{
    blank_lines, eol, header_name, hspace, hspace1, ident, reference, template,
};
```

```rust
fn command() -> Parser<StatementKind> {
    let parameter = hspace1().ignore_left(ident());

    return Parser::<char>::char('#')
        .ignore_left(exact_ident("command"))
        .ignore_left(hspace1())
        .ignore_left(ident().label("a command name"))
        .then(parameter.many())
        .ignore_right(eol())
        .map(|(name, parameters)| StatementKind::Command(Command { name, parameters }));
}

fn assign() -> Parser<StatementKind> {
    return Parser::<char>::char('@')
        .ignore_left(ident())
        .ignore_right(hspace())
        .ignore_right(Parser::<char>::char('='))
        .ignore_right(hspace())
        .then(reference().label("a reference"))
        .ignore_right(eol())
        .map(|(name, value)| StatementKind::Assign(Assign { name, value }));
}
```

Then extend the `choice` list in `statement()`:

```rust
    let kind = Parser::<StatementKind>::choice(vec![host(), headers(), command(), assign()])
        .label("a Nova statement");
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::statement`
Expected: 10 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/statement.rs
git commit -m "feat: Parse #command declarations and @ variable assignments"
```

---

## Task 8: Statements — requests

**Files:**
- Modify: `src/parser/nova/statement.rs`

- [ ] **Step 1: Write the failing tests**

Add inside the existing `mod tests` block in `src/parser/nova/statement.rs`:

```rust
    use crate::parser::nova::value::NovaValue;

    #[test]
    fn parses_an_unlabelled_request() {
        assert_eq!(
            kind("GET /me\n"),
            StatementKind::Request(Request {
                name: None,
                command: None,
                method: Method::Get,
                path: literal("/me"),
                body: None,
            })
        );
    }

    #[test]
    fn parses_a_labelled_request_with_a_body() {
        assert_eq!(
            kind("@login\nPOST /login\n{ \"a\": 1 }\n"),
            StatementKind::Request(Request {
                name: Some("login".to_string()),
                command: None,
                method: Method::Post,
                path: literal("/login"),
                body: Some(NovaValue::Object(vec![(
                    "a".to_string(),
                    NovaValue::Number(1.0)
                )])),
            })
        );
    }

    #[test]
    fn a_dotted_label_carries_a_command_tag() {
        assert_eq!(
            kind("@login.auth\nPOST /login\n"),
            StatementKind::Request(Request {
                name: Some("login".to_string()),
                command: Some("auth".to_string()),
                method: Method::Post,
                path: literal("/login"),
                body: None,
            })
        );
    }

    #[test]
    fn parses_every_method() {
        assert_eq!(method().parse("GET "), Ok((Method::Get, " ")));
        assert_eq!(method().parse("POST "), Ok((Method::Post, " ")));
        assert_eq!(method().parse("PUT "), Ok((Method::Put, " ")));
        assert_eq!(method().parse("PATCH "), Ok((Method::Patch, " ")));
        assert_eq!(method().parse("DELETE "), Ok((Method::Delete, " ")));
        assert_eq!(method().parse("HEAD "), Ok((Method::Head, " ")));
        assert_eq!(method().parse("OPTIONS "), Ok((Method::Options, " ")));
    }

    #[test]
    fn methods_are_uppercase_only() {
        assert!(statement().parse("get /me\n").is_err());
    }

    #[test]
    fn a_label_may_not_use_a_reserved_word() {
        assert!(statement().parse("@env\nGET /me\n").is_err());
    }

    #[test]
    fn a_label_may_not_have_three_segments() {
        assert!(statement().parse("@login.auth.extra\nPOST /x\n").is_err());
    }

    /// A body is optional, so the parser has to peek past the blank line to
    /// decide whether one is there. It must not mistake the next statement for
    /// one. The separating newline is left behind for `parse_nova` to consume.
    #[test]
    fn a_following_statement_is_not_swallowed_as_a_body() {
        let (parsed, remaining) = statement()
            .parse("GET /me\n\n@assert.me.hasField [ \"email\" ]\n")
            .expect("statement should parse");

        match parsed.kind {
            StatementKind::Request(request) => assert_eq!(request.body, None),
            other => panic!("expected a request, got {other:?}"),
        }
        assert_eq!(remaining, "\n@assert.me.hasField [ \"email\" ]\n");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::statement`
Expected: compile error — `method` not found.

- [ ] **Step 3: Write the implementation**

Add to `src/parser/nova/statement.rs` above `mod tests`, and add the import
`use crate::parser::nova::value::{body, NovaValue};`:

```rust
/// A request: an optional label line, a method-and-path line, an optional body.
///
/// Tied with a lazy knot because the body parser is itself recursive.
fn request() -> Parser<StatementKind> {
    return Parser::<StatementKind>::new(|input| request_impl().parse(input));
}

fn request_impl() -> Parser<StatementKind> {
    let labelled = label().ignore_right(eol()).ignore_right(blank_lines());
    let body_line = body().ignore_right(eol());

    return Parser::<(String, Option<String>)>::opt(labelled)
        .then(request_line())
        .then(Parser::<NovaValue>::opt(body_line))
        .map(|((label, (method, path)), body)| {
            let (name, command) = match label {
                Some((name, command)) => (Some(name), command),
                None => (None, None),
            };

            return StatementKind::Request(Request { name, command, method, path, body });
        });
}

/// `@login` or `@login.auth`.
fn label() -> Parser<(String, Option<String>)> {
    let tag = Parser::<String>::opt(Parser::<char>::char('.').ignore_left(ident()));

    return Parser::<char>::char('@')
        .ignore_left(request_ident())
        .then(tag)
        .ignore_right(no_further_segment());
}

/// A request name, rejecting the words the grammar reserves for blocks.
/// Without this, `@env` would be read as a request label.
fn request_ident() -> Parser<String> {
    const RESERVED: [&str; 4] = ["host", "header", "assert", "env"];

    return Parser::<String>::new(|input| {
        let (found, remaining) = ident().parse(input)?;

        if RESERVED.contains(&found.as_str()) {
            return Err(ParseError::new(
                input,
                format!("{found:?} is reserved and cannot name a request"),
            ));
        }

        return Ok((found, remaining));
    });
}

fn no_further_segment() -> Parser<()> {
    return Parser::<()>::new(|input| {
        if input.starts_with('.') {
            return Err(ParseError::new(
                input,
                "A request label takes a name and an optional command",
            ));
        }

        return Ok(((), input));
    });
}

fn request_line() -> Parser<(Method, Template)> {
    return method()
        .ignore_right(hspace1())
        .then(template().label("a request path"))
        .ignore_right(eol());
}

fn method() -> Parser<Method> {
    let verbs = vec![
        verb("GET", Method::Get),
        verb("POST", Method::Post),
        verb("PUT", Method::Put),
        verb("PATCH", Method::Patch),
        verb("DELETE", Method::Delete),
        verb("HEAD", Method::Head),
        verb("OPTIONS", Method::Options),
    ];

    return Parser::<Method>::choice(verbs).label("an HTTP method");
}

fn verb(word: &'static str, method: Method) -> Parser<Method> {
    return Parser::<char>::string(word.to_string()).apply_return(method);
}
```

Make `method` visible to the test module by leaving it private — the tests are
a child module and can see private items.

Then extend the `choice` list in `statement()`. **Order matters:** `request`
must come last, because its label is the most permissive `@` form and would
otherwise shadow the others.

```rust
    let kind = Parser::<StatementKind>::choice(vec![
        host(),
        headers(),
        command(),
        assign(),
        request(),
    ])
    .label("a Nova statement");
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::statement`
Expected: 18 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/statement.rs
git commit -m "feat: Parse requests with labels, command tags and bodies"
```

---

## Task 9: Statements — assertions

**Files:**
- Modify: `src/parser/nova/statement.rs`

- [ ] **Step 1: Write the failing tests**

Add inside the existing `mod tests` block in `src/parser/nova/statement.rs`:

```rust
    #[test]
    fn parses_type_only_assertions() {
        assert_eq!(
            kind("@assert.login.typeOnly\naccessToken: string\n"),
            StatementKind::Assert(Assert {
                request: "login".to_string(),
                assertion: Assertion::TypeOnly(vec![(
                    "accessToken".to_string(),
                    TypeName::String
                )]),
            })
        );
    }

    #[test]
    fn parses_every_type_name() {
        let source = "@assert.r.typeOnly\na: string\nb: number\nc: boolean\nd: array\ne: object\nf: null\n";

        assert_eq!(
            kind(source),
            StatementKind::Assert(Assert {
                request: "r".to_string(),
                assertion: Assertion::TypeOnly(vec![
                    ("a".to_string(), TypeName::String),
                    ("b".to_string(), TypeName::Number),
                    ("c".to_string(), TypeName::Boolean),
                    ("d".to_string(), TypeName::Array),
                    ("e".to_string(), TypeName::Object),
                    ("f".to_string(), TypeName::Null),
                ]),
            })
        );
    }

    #[test]
    fn parses_inline_field_lists() {
        assert_eq!(
            kind("@assert.me.hasField [ \"email\" ]\n"),
            StatementKind::Assert(Assert {
                request: "me".to_string(),
                assertion: Assertion::HasField(vec!["email".to_string()]),
            })
        );
        assert_eq!(
            kind("@assert.me.exactFields [ \"email\", \"user_id\" ]\n"),
            StatementKind::Assert(Assert {
                request: "me".to_string(),
                assertion: Assertion::ExactFields(vec![
                    "email".to_string(),
                    "user_id".to_string()
                ]),
            })
        );
    }

    #[test]
    fn parses_value_matching_assertions() {
        assert_eq!(
            kind("@assert.me.fieldMatch\nemail: \"john@example.com\"\n"),
            StatementKind::Assert(Assert {
                request: "me".to_string(),
                assertion: Assertion::FieldMatch(vec![(
                    "email".to_string(),
                    NovaValue::String("john@example.com".to_string())
                )]),
            })
        );
        assert_eq!(
            kind("@assert.me.exactMatch\nemail: \"a\"\nuser_id: \"1\"\n"),
            StatementKind::Assert(Assert {
                request: "me".to_string(),
                assertion: Assertion::ExactMatch(vec![
                    ("email".to_string(), NovaValue::String("a".to_string())),
                    ("user_id".to_string(), NovaValue::String("1".to_string())),
                ]),
            })
        );
    }

    #[test]
    fn rejects_an_unknown_assertion_mode() {
        assert!(statement().parse("@assert.me.nonsense\na: 1\n").is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::statement`
Expected: 5 failures — `statement should parse` panics.

- [ ] **Step 3: Write the implementation**

Add to `src/parser/nova/statement.rs` above `mod tests`, and extend the value
import to `use crate::parser::nova::value::{body, line_value, string_array, NovaValue};`:

```rust
fn assert() -> Parser<StatementKind> {
    let dot = Parser::<char>::char('.');

    return marker("assert")
        .ignore_left(dot.clone())
        .ignore_left(ident().label("a request name"))
        .ignore_right(dot)
        .then(assertion())
        .map(|(request, assertion)| StatementKind::Assert(Assert { request, assertion }));
}

/// The mode word decides the shape, so no lookahead is needed: `hasField` and
/// `exactFields` take their list inline, the rest take member lines below.
fn assertion() -> Parser<Assertion> {
    let modes = vec![
        inline_mode("hasField", Assertion::HasField),
        inline_mode("exactFields", Assertion::ExactFields),
        block_mode("typeOnly", type_line(), Assertion::TypeOnly),
        block_mode("fieldMatch", match_line(), Assertion::FieldMatch),
        block_mode("exactMatch", match_line(), Assertion::ExactMatch),
    ];

    return Parser::<Assertion>::choice(modes).label("an assertion mode");
}

fn inline_mode<F>(word: &'static str, build: F) -> Parser<Assertion>
where
    F: Fn(Vec<String>) -> Assertion + 'static,
{
    return exact_ident(word)
        .ignore_right(hspace())
        .ignore_left(string_array())
        .ignore_right(eol())
        .map(build);
}

fn block_mode<T, F>(word: &'static str, member: Parser<T>, build: F) -> Parser<Assertion>
where
    T: 'static,
    F: Fn(Vec<T>) -> Assertion + 'static,
{
    return exact_ident(word)
        .ignore_left(eol())
        .ignore_left(blank_lines())
        .ignore_left(member.ignore_right(blank_lines()).many1())
        .map(build);
}

fn type_line() -> Parser<(String, TypeName)> {
    return ident()
        .ignore_right(Parser::<char>::char(':'))
        .ignore_right(hspace())
        .then(type_name())
        .ignore_right(eol());
}

fn type_name() -> Parser<TypeName> {
    let names = vec![
        named_type("string", TypeName::String),
        named_type("number", TypeName::Number),
        named_type("boolean", TypeName::Boolean),
        named_type("array", TypeName::Array),
        named_type("object", TypeName::Object),
        named_type("null", TypeName::Null),
    ];

    return Parser::<TypeName>::choice(names).label("a type name");
}

fn named_type(word: &'static str, name: TypeName) -> Parser<TypeName> {
    return crate::parser::json_parser::keyword(word).apply_return(name);
}

fn match_line() -> Parser<(String, NovaValue)> {
    return ident()
        .ignore_right(Parser::<char>::char(':'))
        .ignore_right(hspace())
        .then(line_value())
        .ignore_right(eol());
}
```

Then extend the `choice` list in `statement()`. `assert` must come before
`assign` and `request`, since `@assert...` would otherwise be read as a label:

```rust
    let kind = Parser::<StatementKind>::choice(vec![
        host(),
        headers(),
        command(),
        assert(),
        assign(),
        request(),
    ])
    .label("a Nova statement");
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::statement`
Expected: 23 passed.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/statement.rs
git commit -m "feat: Parse all five assertion forms"
```

---

## Task 10: The document entry point

**Files:**
- Modify: `src/parser/nova/mod.rs`

- [ ] **Step 1: Write the failing tests**

Append to `src/parser/nova/mod.rs`:

```rust
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test parser::nova::tests`
Expected: compile error — `parse_nova`, `line_column` not found.

- [ ] **Step 3: Write the implementation**

Replace the contents of `src/parser/nova/mod.rs` above `mod tests` with:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::tests`
Expected: 6 passed.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test`
Expected: all pass, including the 12 original `json_parser` tests.

- [ ] **Step 6: Commit**

```bash
git add src/parser/nova/mod.rs
git commit -m "feat: Add parse_nova entry point and line_column reporting"
```

---

## Task 11: Acceptance tests — the eight specification examples

**Files:**
- Create: `src/parser/nova/acceptance.rs`
- Modify: `src/parser/nova/mod.rs`

This is the acceptance criterion from the spec: all eight examples from the
language specification, verbatim, parsed as whole documents. These assert
*structure* — statement counts and kinds — rather than every field, because
the per-field assertions already live in Tasks 6-9. What these catch is the
statements failing to compose.

- [ ] **Step 1: Create the test file**

Create `src/parser/nova/acceptance.rs`:

```rust
//! The eight examples from the Nova language specification, parsed verbatim.
//!
//! If these pass, the parser handles the language as documented.

#![cfg(test)]

use crate::parser::nova::ast::*;
use crate::parser::nova::parse_nova;

fn parse(source: &str) -> Document {
    return match parse_nova(source) {
        Ok(document) => document,
        Err(error) => {
            let (line, column) = crate::parser::nova::line_column(source, error.position(source));
            panic!("failed to parse at line {line}, column {column}: {error}");
        }
    };
}

#[test]
fn example_1_host_only() {
    let document = parse("@host\nhttp://localhost:3000\n");

    assert_eq!(document.statements.len(), 1);
    assert!(matches!(document.statements[0].kind, StatementKind::Host(_)));
}

#[test]
fn example_2_host_and_default_header() {
    let document = parse(
        "@host\nhttp://localhost:3000\n\n@header\nContent-Type: application/json\n",
    );

    assert_eq!(document.statements.len(), 2);
    assert!(matches!(document.statements[0].kind, StatementKind::Host(_)));

    match &document.statements[1].kind {
        StatementKind::Headers(headers) => {
            assert_eq!(headers.len(), 1);
            assert_eq!(headers[0].name, "Content-Type");
        }
        other => panic!("expected headers, got {other:?}"),
    }
}

#[test]
fn example_3_named_request_with_a_literal_body() {
    let document = parse(
        "@login\nPOST /login\n{\n    \"email\": \"john@example.com\",\n    \"password\": \"12345678\",\n}\n",
    );

    assert_eq!(document.statements.len(), 1);

    match &document.statements[0].kind {
        StatementKind::Request(request) => {
            assert_eq!(request.name.as_deref(), Some("login"));
            assert_eq!(request.method, Method::Post);
            assert!(request.body.is_some(), "trailing comma should be accepted");
        }
        other => panic!("expected a request, got {other:?}"),
    }
}

#[test]
fn example_4_body_referring_to_the_environment() {
    let document = parse(
        "@login\nPOST /login\n{\n    \"email\": @env.EMAIL,\n    \"password\": @env.PASSWORD\n}\n",
    );

    assert_eq!(document.statements.len(), 1);

    match &document.statements[0].kind {
        StatementKind::Request(request) => match request.body.as_ref().expect("a body") {
            crate::parser::nova::NovaValue::Object(members) => {
                assert_eq!(members.len(), 2);
                assert!(matches!(members[0].1, crate::parser::nova::NovaValue::Ref(_)));
            }
            other => panic!("expected an object body, got {other:?}"),
        },
        other => panic!("expected a request, got {other:?}"),
    }
}

#[test]
fn example_5_assignment_then_header_then_request() {
    let document = parse(concat!(
        "@login\n",
        "POST /login\n",
        "{\n",
        "    \"email\": @env.EMAIL,\n",
        "    \"password\": @env.PASSWORD\n",
        "}\n",
        "\n",
        "@accessToken = @login.response.body.accessToken\n",
        "\n",
        "@header\n",
        "Authorization: Bearer @accessToken\n",
        "\n",
        "GET /me\n",
    ));

    assert_eq!(document.statements.len(), 4);
    assert!(matches!(document.statements[0].kind, StatementKind::Request(_)));
    assert!(matches!(document.statements[1].kind, StatementKind::Assign(_)));
    assert!(matches!(document.statements[2].kind, StatementKind::Headers(_)));
    assert!(matches!(document.statements[3].kind, StatementKind::Request(_)));
}

#[test]
fn example_6_type_only_assertion() {
    let document = parse(concat!(
        "@login\n",
        "POST /login\n",
        "{\n",
        "    \"email\": @env.EMAIL,\n",
        "    \"password\": @env.PASSWORD\n",
        "}\n",
        "\n",
        "@assert.login.typeOnly\n",
        "accessToken: string\n",
    ));

    assert_eq!(document.statements.len(), 2);

    match &document.statements[1].kind {
        StatementKind::Assert(assertion) => {
            assert_eq!(assertion.request, "login");
            assert_eq!(
                assertion.assertion,
                Assertion::TypeOnly(vec![("accessToken".to_string(), TypeName::String)])
            );
        }
        other => panic!("expected an assertion, got {other:?}"),
    }
}

#[test]
fn example_7_every_assertion_form() {
    let document = parse(concat!(
        "@accessToken = @login.response.body.accessToken\n",
        "\n",
        "@header\n",
        "Authorization: Bearer @accessToken\n",
        "\n",
        "@me\n",
        "GET /me\n",
        "\n",
        "@assert.me.hasField [ \"email\" ]\n",
        "\n",
        "@assert.me.exactFields [ \"email\", \"user_id\" ]\n",
        "\n",
        "@assert.me.fieldMatch\n",
        "email: \"john@example.com\"\n",
        "\n",
        "@assert.me.exactMatch\n",
        "email: \"john@example.com\"\n",
        "user_id: \"1\"\n",
    ));

    assert_eq!(document.statements.len(), 7);

    let assertions: Vec<&Assertion> = document
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Assert(assertion) => Some(&assertion.assertion),
            _ => None,
        })
        .collect();

    assert_eq!(assertions.len(), 4);
    assert!(matches!(assertions[0], Assertion::HasField(_)));
    assert!(matches!(assertions[1], Assertion::ExactFields(_)));
    assert!(matches!(assertions[2], Assertion::FieldMatch(_)));
    assert!(matches!(assertions[3], Assertion::ExactMatch(_)));
}

#[test]
fn example_8_command_with_tagged_requests() {
    let document = parse(concat!(
        "#command auth email password\n",
        "\n",
        "@login.auth\n",
        "POST /login\n",
        "{\n",
        "    \"email\": #auth.email,\n",
        "    \"password\": #auth.password,\n",
        "}\n",
        "\n",
        "@me.auth\n",
        "GET /me\n",
    ));

    assert_eq!(document.statements.len(), 3);

    match &document.statements[0].kind {
        StatementKind::Command(command) => {
            assert_eq!(command.name, "auth");
            assert_eq!(command.parameters, vec!["email".to_string(), "password".to_string()]);
        }
        other => panic!("expected a command, got {other:?}"),
    }

    let tagged: Vec<(Option<&str>, Option<&str>)> = document
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            StatementKind::Request(request) => {
                Some((request.name.as_deref(), request.command.as_deref()))
            }
            _ => None,
        })
        .collect();

    assert_eq!(tagged, vec![(Some("login"), Some("auth")), (Some("me"), Some("auth"))]);
}
```

- [ ] **Step 2: Register the module**

Add to `src/parser/nova/mod.rs`, after the other `pub mod` lines:

```rust
mod acceptance;
```

- [ ] **Step 3: Run the acceptance tests**

Run: `cargo test parser::nova::acceptance`
Expected: 8 passed. Failures print a line and column — use them.

- [ ] **Step 4: Run the whole suite**

Run: `cargo test`
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/parser/nova/acceptance.rs src/parser/nova/mod.rs
git commit -m "test: Add acceptance tests for the eight specification examples"
```

---

## Task 12: Error message quality

**Files:**
- Modify: `src/parser/nova/mod.rs`

The spec lists six error cases that must produce a message naming the
construct. `json_parser.rs` has an equivalent test — follow its shape.

- [ ] **Step 1: Write the failing tests**

Add inside the existing `mod tests` block in `src/parser/nova/mod.rs`:

```rust
    fn error_for(input: &str) -> String {
        return parse_nova(input).expect_err("input should not parse").message;
    }

    #[test]
    fn errors_name_the_construct_that_failed() {
        assert!(error_for("get /me\n").contains("HTTP method"), "{}", error_for("get /me\n"));
        assert!(
            error_for("@login.auth.extra\nPOST /x\n").contains("name and an optional command"),
            "{}",
            error_for("@login.auth.extra\nPOST /x\n")
        );
        assert!(
            error_for("@assert.me.nonsense\na: 1\n").contains("assertion mode"),
            "{}",
            error_for("@assert.me.nonsense\na: 1\n")
        );
        assert!(error_for("@\nGET /x\n").contains("identifier"), "{}", error_for("@\nGET /x\n"));
        assert!(
            error_for("POST /x\n{ \"a\": 1\n").contains('}'),
            "{}",
            error_for("POST /x\n{ \"a\": 1\n")
        );
        assert_eq!(error_for("GET /me\n!!!\n"), "Expected end of input");
    }

    #[test]
    fn a_reserved_word_cannot_name_a_request() {
        assert!(error_for("@env\nGET /me\n").contains("reserved"), "{}", error_for("@env\nGET /me\n"));
    }
```

- [ ] **Step 2: Run the tests**

Run: `cargo test parser::nova::tests`
Expected: some assertions fail, printing the actual message.

- [ ] **Step 3: Adjust labels until each message names its construct**

Do not change the grammar. Only adjust the `.label(..)` strings and the
`ParseError::new` messages already written in Tasks 4, 6, 8 and 9 so that each
assertion above holds. The `best`-error logic in `choice` keeps the deepest
failure, so a message that looks wrong usually means a parser committed further
than expected — check which alternative got furthest before editing.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test parser::nova::tests`
Expected: 8 passed.

- [ ] **Step 5: Run the whole suite one final time**

Run: `cargo test`
Expected: all pass — 12 `json_parser`, 13 `lex`, 10 `value`, 23 `statement`, 8 `mod`, 8 `acceptance`.

- [ ] **Step 6: Commit**

```bash
git add src/parser/nova/
git commit -m "test: Assert error messages name the construct that failed"
```

---

## Done

At this point `parse_nova` handles the whole language as specified. Not built,
and deliberately so — these belong to later work:

- Resolving references, and reporting undefined ones
- Folding `@host` / `@header` context into requests
- The HTTP execution engine and assertion evaluation
- Updating `README.md`, which still documents the older syntax
