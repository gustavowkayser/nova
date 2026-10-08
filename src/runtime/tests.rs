use std::cell::RefCell;

use super::*;
use crate::parser::nova::parse_nova;

struct Sent {
    method: Method,
    url: String,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
}

struct Fake {
    sent: RefCell<Vec<Sent>>,
    replies: RefCell<Vec<Result<HttpResponse, ErrorKind>>>,
}

impl Fake {
    fn new(replies: Vec<Result<HttpResponse, ErrorKind>>) -> Self {
        Self {
            sent: RefCell::new(Vec::new()),
            replies: RefCell::new(replies),
        }
    }
}

impl Transport for Fake {
    fn send(
        &self,
        request: &PreparedRequest,
        _options: &RunOptions,
    ) -> Result<HttpResponse, ErrorKind> {
        self.sent.borrow_mut().push(Sent {
            method: request.method,
            url: request.url.to_string(),
            headers: request.headers.clone(),
            body: request.body.clone(),
        });
        self.replies.borrow_mut().remove(0)
    }
}

fn response(status: u16, body: &[u8]) -> Result<HttpResponse, ErrorKind> {
    Ok(HttpResponse {
        status,
        headers: vec![("x-result".into(), vec![0xff])],
        body: body.to_vec(),
    })
}

fn run_fake(source: &str, fake: &Fake) -> Result<RunReport, ExecutionFailure> {
    let document = parse_nova(source).unwrap();
    let prepared = prepare(&document)?;
    run(prepared, fake, &RunOptions::default())
}

#[test]
fn sends_requests_in_order_with_positional_host_and_headers() {
    let source = "@host\nhttp://example.com/api\n@header\nX-Key: one\nx-key: two\nGET /users?active=true\n@host\nhttps://example.com/v2/\n@header\nOther: yes\n@created\nPOST /items\n{\"name\":\"a\\\"b\", \"list\":[true,null,2,],}\n";
    let fake = Fake::new(vec![response(302, b"redirect"), response(404, &[0, 255])]);
    let report = run_fake(source, &fake).unwrap();
    let sent = fake.sent.borrow();

    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].url, "http://example.com/api/users?active=true");
    assert_eq!(sent[0].headers.get("x-key").unwrap(), "two");
    assert!(!sent[0].headers.contains_key(CONTENT_TYPE));
    assert_eq!(sent[1].url, "https://example.com/v2/items");
    assert!(!sent[1].headers.contains_key("x-key"));
    assert_eq!(sent[1].headers.get("other").unwrap(), "yes");
    assert_eq!(
        sent[1].headers.get(CONTENT_TYPE).unwrap(),
        "application/json"
    );
    assert_eq!(sent[1].method, Method::Post);
    assert!(String::from_utf8_lossy(sent[1].body.as_ref().unwrap()).contains("[true,null,2]"));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(sent[1].body.as_ref().unwrap()).unwrap(),
        serde_json::json!({"name":"a\"b", "list":[true,null,2]})
    );
    assert_eq!(report.requests[0].status, 302);
    assert_eq!(report.requests[1].status, 404);
    assert_eq!(report.requests[1].name.as_deref(), Some("created"));
    assert_eq!(report.requests[1].body, vec![0, 255]);
    assert_eq!(report.requests[1].headers[0].1, vec![0xff]);
}

#[test]
fn preserves_path_syntax_and_rejects_encoded_prefix_escapes() {
    let fake = Fake::new(vec![response(200, b""), response(200, b"")]);
    run_fake(
        "@host\nhttp://example.com/api\nGET /foo:bar\nGET /http://example.com/api/secret\n",
        &fake,
    )
    .unwrap();
    let sent = fake.sent.borrow();
    assert_eq!(sent[0].url, "http://example.com/api/foo:bar");
    assert_eq!(
        sent[1].url,
        "http://example.com/api/http://example.com/api/secret"
    );

    for path in [
        "/..%2Fadmin",
        "/%2e%2e/admin",
        "/a%5cb",
        "/a%252fb",
        "/a\\b",
    ] {
        let source = format!("@host\nhttp://example.com/api\nGET {path}\n");
        let document = parse_nova(&source).unwrap();
        let failure = prepare(&document).err().expect(&source);
        assert!(
            matches!(failure.kind, ErrorKind::InvalidPath(_)),
            "{path}: {:?}",
            failure.kind
        );
    }
}

#[test]
fn integer_json_literals_remain_integers_on_the_wire() {
    let fake = Fake::new(vec![response(200, b"")]);
    run_fake(
        "@host\nhttp://example.com\nPOST /x\n{\"id\":1,\"fraction\":2.5}\n",
        &fake,
    )
    .unwrap();
    assert_eq!(
        fake.sent.borrow()[0].body.as_deref(),
        Some(br#"{"id":1,"fraction":2.5}"#.as_slice())
    );
}

#[test]
fn supports_every_method_and_an_explicit_content_type() {
    let mut source =
        "@host\nhttp://example.com/base\n@header\nContent-Type: application/custom\n".to_string();
    for verb in ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"] {
        source.push_str(&format!("{verb} /x\n"));
    }
    let fake = Fake::new((0..7).map(|_| response(500, b"error")).collect());
    let report = run_fake(&source, &fake).unwrap();
    assert_eq!(report.requests.len(), 7);
    assert!(report.requests.iter().all(|record| record.status == 500));
    for request in fake.sent.borrow().iter() {
        assert_eq!(request.url, "http://example.com/base/x");
        assert_eq!(
            request.headers.get(CONTENT_TYPE).unwrap(),
            "application/custom"
        );
    }
}

#[test]
fn rejects_unsupported_features_before_any_send() {
    let additions = [
        "@host\n@env.URL\nGET /x\n",
        "@host\nhttp://example.com\nGET /@id\n",
        "@host\nhttp://example.com\n@header\nAuthorization: Bearer @token\nGET /x\n",
        "@host\nhttp://example.com\nPOST /x\n{\"nested\":[@env.ID]}\n",
        "@host\nhttp://example.com\nGET /x\n@name = @env.X\n",
        "@host\nhttp://example.com\n@item\nGET /x\n@assert.item.hasField [\"id\"]\n",
        "@host\nhttp://example.com\nGET /x\n#command auth user\n",
        "@host\nhttp://example.com\nGET /x\n@item.auth\nGET /x\n",
    ];
    for source in additions {
        let fake = Fake::new(vec![]);
        let failure = run_fake(source, &fake).err().expect(source);
        assert!(
            matches!(failure.kind, ErrorKind::UnsupportedFeature(_)),
            "{source}: {:?}",
            failure.kind
        );
        assert_eq!(failure.completed.requests.len(), 0);
        assert!(fake.sent.borrow().is_empty());
    }
}

#[test]
fn rejects_invalid_configuration_in_preflight() {
    let examples = [
        ("GET /x\n", "missing"),
        ("@host\nftp://example.com\nGET /x\n", "host"),
        ("@host\nhttp://example.com?x=1\nGET /x\n", "host"),
        ("@host\nhttp://example.com/base\nGET /../outside\n", "path"),
        (
            "@host\nhttp://example.com\nPOST /x\n{\"id\":9007199254740993}\n",
            "body",
        ),
    ];
    for (source, category) in examples {
        let fake = Fake::new(vec![]);
        let failure = run_fake(source, &fake).err().expect(source);
        assert_eq!(failure.completed.requests.len(), 0);
        assert!(fake.sent.borrow().is_empty());
        assert!(
            match category {
                "missing" => matches!(failure.kind, ErrorKind::MissingHost),
                "host" => matches!(failure.kind, ErrorKind::InvalidHost(_)),
                "path" => matches!(failure.kind, ErrorKind::InvalidPath(_)),
                _ => matches!(failure.kind, ErrorKind::InvalidBody(_)),
            },
            "{source}: {:?}",
            failure.kind
        );
    }
    let mut document =
        parse_nova("@host\nhttp://example.com\n@header\nGood: value\nGET /x\n").unwrap();
    if let StatementKind::Headers(fields) = &mut document.statements[1].kind {
        fields[0].name = "invalid name".into();
    }
    assert!(matches!(
        prepare(&document).err().unwrap().kind,
        ErrorKind::InvalidHeader(_)
    ));
    assert!(matches!(
        request_url(
            &Url::parse("http://example.com/base/").unwrap(),
            "//another-host/x"
        ),
        Err(ErrorKind::InvalidPath(_))
    ));
}

#[test]
fn preserves_partial_results_and_statement_offset_on_transport_error() {
    let source = "@host\nhttp://example.com\nGET /ok\nGET /broken\n";
    let fake = Fake::new(vec![
        response(200, b"ok"),
        Err(ErrorKind::Timeout("timed out".into())),
    ]);
    let failure = run_fake(source, &fake).err().unwrap();
    assert_eq!(failure.offset, source.find("GET /broken").unwrap());
    assert_eq!(failure.completed.requests.len(), 1);
    assert_eq!(failure.completed.requests[0].body, b"ok");
    assert!(matches!(failure.kind, ErrorKind::Timeout(_)));
}

#[test]
fn a_document_without_requests_succeeds() {
    let document = parse_nova("@host\nhttp://example.com\n").unwrap();
    assert!(
        execute(&document, &RunOptions::default())
            .unwrap()
            .requests
            .is_empty()
    );
}

#[test]
fn encodes_quoted_references_as_strings_and_rejects_non_finite_values() {
    let body = NovaValue::Object(vec![("key".into(), NovaValue::String("@env.KEY".into()))]);
    assert_eq!(encode_body(&body).unwrap(), br#"{"key":"@env.KEY"}"#);
    assert!(matches!(
        encode_body(&NovaValue::Number(f64::INFINITY)),
        Err(ErrorKind::InvalidBody(_))
    ));
}

#[test]
fn response_body_limit_reports_the_request_offset() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc")
            .unwrap();
    });
    let source = format!("@host\nhttp://127.0.0.1:{port}\nGET /large\n");
    let document = parse_nova(&source).unwrap();
    let options = RunOptions {
        max_response_bytes: 2,
        ..RunOptions::default()
    };
    let failure = execute(&document, &options).err().unwrap();
    server.join().unwrap();
    assert_eq!(failure.offset, source.find("GET /large").unwrap());
    assert!(matches!(
        failure.kind,
        ErrorKind::ResponseTooLarge { limit: 2 }
    ));
}

#[test]
fn response_body_read_timeout_is_classified_as_timeout() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(150));
    });
    let source = format!("@host\nhttp://127.0.0.1:{port}\nGET /slow\n");
    let document = parse_nova(&source).unwrap();
    let options = RunOptions {
        timeout: Duration::from_millis(30),
        ..RunOptions::default()
    };
    let failure = execute(&document, &options).err().unwrap();
    server.join().unwrap();
    assert_eq!(failure.offset, source.find("GET /slow").unwrap());
    assert!(
        matches!(failure.kind, ErrorKind::Timeout(_)),
        "{:?}",
        failure.kind
    );
}

#[test]
fn trickling_response_cannot_extend_the_total_deadline() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0; 4096];
        stream.read(&mut request).unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n")
            .unwrap();
        for _ in 0..6 {
            std::thread::sleep(Duration::from_millis(40));
            if stream.write_all(b"x").is_err() {
                break;
            }
        }
    });
    let source = format!("@host\nhttp://127.0.0.1:{port}\nGET /slow\n");
    let document = parse_nova(&source).unwrap();
    let options = RunOptions {
        timeout: Duration::from_millis(90),
        ..RunOptions::default()
    };
    let failure = execute(&document, &options).err().unwrap();
    server.join().unwrap();
    assert!(
        matches!(failure.kind, ErrorKind::Timeout(_)),
        "{:?}",
        failure.kind
    );
}

#[test]
fn local_server_receives_get_and_json_post() {
    use std::io::{Read, Write};
    use std::net::TcpListener;

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        for expected in ["GET /api/items HTTP/1.1", "POST /api/items HTTP/1.1"] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let header_end = loop {
                let mut chunk = [0; 4096];
                let count = stream.read(&mut chunk).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&chunk[..count]);
                if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            assert!(headers.starts_with(expected), "{headers}");
            if expected.starts_with("POST") {
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|n| n.parse::<usize>().ok())
                    })
                    .unwrap();
                while bytes.len() - header_end < length {
                    let mut chunk = [0; 4096];
                    let count = stream.read(&mut chunk).unwrap();
                    assert!(count > 0);
                    bytes.extend_from_slice(&chunk[..count]);
                }
                assert_eq!(
                    serde_json::from_slice::<serde_json::Value>(&bytes[header_end..]).unwrap(),
                    serde_json::json!({"ok":true})
                );
            }
            stream
                .write_all(
                    b"HTTP/1.1 201 Created\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                )
                .unwrap();
        }
    });

    let source =
        format!("@host\nhttp://127.0.0.1:{port}/api\nGET /items\nPOST /items\n{{\"ok\":true}}\n");
    let document = parse_nova(&source).unwrap();
    let report = execute(&document, &RunOptions::default()).unwrap();
    server.join().unwrap();
    assert_eq!(report.requests.len(), 2);
    assert!(
        report
            .requests
            .iter()
            .all(|response| response.status == 201 && response.body == b"ok")
    );
}
