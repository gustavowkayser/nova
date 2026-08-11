use crate::parser::nova::ast::*;
use crate::parser::nova::lex::{
    blank_lines, eol, header_name, hspace, hspace1, ident, reference, template,
};
use crate::parser::nova::value::{body, line_value, starts_body, string_array, NovaValue};
use crate::parser::parser::{ParseError, Parser};

/// One statement, tagged with where it started.
///
/// `offset` is recorded as *bytes remaining* here, because a parser only ever
/// sees a suffix of the document and cannot know its own absolute position.
/// [`super::parse_nova`] converts these to real offsets once it knows the
/// total length.
pub fn statement() -> Parser<Statement> {
    // `request` goes first for the sake of error messages, not correctness.
    // Every alternative fails at offset zero on input that starts no statement
    // at all, and `choice` keeps the earliest of equally-deep failures — so
    // whichever branch leads decides the message. "Expected an HTTP method" is
    // the most actionable of them. It is safe to lead with the most permissive
    // form because the reserved words (`host`, `header`, `assert`, `env`) are
    // what stop a label from swallowing the block forms.
    //
    // Deliberately unlabelled: `label` replaces any failure that consumed
    // nothing, which is precisely this case, and "Expected a Nova statement"
    // is less useful than the branch's own error.
    let kind = Parser::<StatementKind>::choice(vec![
        request(),
        host(),
        headers(),
        command(),
        assert(),
        assign(),
    ]);

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

/// A request: an optional label line, a method-and-path line, an optional body.
///
/// Tied with a lazy knot because the body parser is itself recursive.
fn request() -> Parser<StatementKind> {
    return Parser::<StatementKind>::new(|input| request_impl().parse(input));
}

fn request_impl() -> Parser<StatementKind> {
    let labelled = label().ignore_right(eol()).ignore_right(blank_lines());
    let body_line = body().ignore_right(eol());

    // Once a line starts with `@` it must be a label — there is no other way
    // for a request to begin that way. Committing matters for error quality:
    // wrapping this in `opt` would discard a malformed label's failure and
    // backtrack to offset zero, leaving the useless "expected an HTTP method"
    // in its place. Other `@` forms are unaffected, since `choice` still goes
    // on to try them.
    let prefix = Parser::<Option<(String, Option<String>)>>::new(move |input| {
        if !input.starts_with('@') {
            return Ok((None, input));
        }

        let (found, remaining) = labelled.parse(input)?;

        return Ok((Some(found), remaining));
    });

    // Committed for the same reason as the label: `opt` would turn "this body
    // is missing its closing brace" into "there was no body here".
    let optional_body = Parser::<Option<NovaValue>>::new(move |input| {
        if !starts_body(input) {
            return Ok((None, input));
        }

        let (found, remaining) = body_line.parse(input)?;

        return Ok((Some(found), remaining));
    });

    return prefix
        .then(request_line())
        .then(optional_body)
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
            // Reported at `remaining`, not `input`: the identifier really was
            // consumed before being rejected, so this is how far the parser
            // got. That also lets the message outrank `assign`'s shallower
            // "expected '='" when both branches fail on `@env`.
            return Err(ParseError::new(
                remaining,
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
}
