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
