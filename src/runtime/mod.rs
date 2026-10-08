//! Execute the literal HTTP subset of a parsed Nova document.
//!
//! References, assertions, assignments and commands are rejected during
//! preflight, before the first request is sent.

mod http;

use std::time::Duration;

use reqwest::header::{CONTENT_TYPE, HeaderMap, HeaderName, HeaderValue};
use url::Url;

use crate::parser::nova::{Document, Method, NovaValue, StatementKind, Template, TemplatePart};
use http::{HttpResponse, ReqwestTransport, Transport};

#[derive(Debug, Clone)]
pub struct RunOptions {
    pub timeout: Duration,
    pub max_response_bytes: usize,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_response_bytes: 10 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Default)]
pub struct RunReport {
    pub requests: Vec<ResponseRecord>,
}

#[derive(Debug)]
pub struct ResponseRecord {
    pub name: Option<String>,
    pub method: Method,
    pub url: Url,
    pub status: u16,
    /// Header values are bytes because HTTP response headers need not be UTF-8.
    pub headers: Vec<(String, Vec<u8>)>,
    pub body: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ErrorKind {
    UnsupportedFeature(String),
    MissingHost,
    InvalidHost(String),
    InvalidPath(String),
    InvalidHeader(String),
    InvalidBody(String),
    Transport(String),
    Timeout(String),
    ResponseTooLarge { limit: usize },
}

#[derive(Debug)]
pub struct ExecutionFailure {
    /// Byte offset of the offending statement in the original `.nova` source.
    pub offset: usize,
    pub kind: ErrorKind,
    pub completed: RunReport,
}

impl std::fmt::Display for ExecutionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Execution failed at byte {}: {:?}",
            self.offset, self.kind
        )
    }
}

impl std::error::Error for ExecutionFailure {}

pub fn execute(document: &Document, options: &RunOptions) -> Result<RunReport, ExecutionFailure> {
    let requests = prepare(document)?;
    if requests.is_empty() {
        return Ok(RunReport::default());
    }

    let transport = ReqwestTransport::new(options).map_err(|kind| ExecutionFailure {
        offset: requests[0].offset,
        kind,
        completed: RunReport::default(),
    })?;
    run(requests, &transport, options)
}

struct PreparedRequest {
    offset: usize,
    name: Option<String>,
    method: Method,
    url: Url,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
}

fn prepare(document: &Document) -> Result<Vec<PreparedRequest>, ExecutionFailure> {
    let mut host: Option<Url> = None;
    let mut headers = HeaderMap::new();
    let mut requests = Vec::new();

    for statement in &document.statements {
        let prepared = match &statement.kind {
            StatementKind::Host(template) => {
                host = Some(
                    parse_host(&literal(template).map_err(|kind| failure(statement.offset, kind))?)
                        .map_err(|kind| failure(statement.offset, kind))?,
                );
                None
            }
            StatementKind::Headers(fields) => {
                let mut next = HeaderMap::new();
                for field in fields {
                    let value =
                        literal(&field.value).map_err(|kind| failure(statement.offset, kind))?;
                    let name = HeaderName::from_bytes(field.name.as_bytes()).map_err(|error| {
                        failure(
                            statement.offset,
                            ErrorKind::InvalidHeader(error.to_string()),
                        )
                    })?;
                    let value = HeaderValue::from_str(&value).map_err(|error| {
                        failure(
                            statement.offset,
                            ErrorKind::InvalidHeader(error.to_string()),
                        )
                    })?;
                    next.insert(name, value);
                }
                headers = next;
                None
            }
            StatementKind::Request(request) => {
                if request.command.is_some() {
                    return Err(failure(
                        statement.offset,
                        ErrorKind::UnsupportedFeature("command-tagged request".into()),
                    ));
                }
                let base = host
                    .as_ref()
                    .ok_or_else(|| failure(statement.offset, ErrorKind::MissingHost))?;
                let path =
                    literal(&request.path).map_err(|kind| failure(statement.offset, kind))?;
                let url =
                    request_url(base, &path).map_err(|kind| failure(statement.offset, kind))?;
                let body = request
                    .body
                    .as_ref()
                    .map(encode_body)
                    .transpose()
                    .map_err(|kind| failure(statement.offset, kind))?;
                let mut request_headers = headers.clone();
                if body.is_some() && !request_headers.contains_key(CONTENT_TYPE) {
                    request_headers
                        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
                }
                Some(PreparedRequest {
                    offset: statement.offset,
                    name: request.name.clone(),
                    method: request.method,
                    url,
                    headers: request_headers,
                    body,
                })
            }
            StatementKind::Assign(_) => {
                return Err(failure(
                    statement.offset,
                    ErrorKind::UnsupportedFeature("assignment".into()),
                ));
            }
            StatementKind::Assert(_) => {
                return Err(failure(
                    statement.offset,
                    ErrorKind::UnsupportedFeature("assertion".into()),
                ));
            }
            StatementKind::Command(_) => {
                return Err(failure(
                    statement.offset,
                    ErrorKind::UnsupportedFeature("command".into()),
                ));
            }
        };
        if let Some(prepared) = prepared {
            requests.push(prepared);
        }
    }

    Ok(requests)
}

fn failure(offset: usize, kind: ErrorKind) -> ExecutionFailure {
    ExecutionFailure {
        offset,
        kind,
        completed: RunReport::default(),
    }
}

fn literal(template: &Template) -> Result<String, ErrorKind> {
    let mut text = String::new();
    for part in &template.parts {
        match part {
            TemplatePart::Literal(value) => text.push_str(value),
            TemplatePart::Ref(_) => {
                return Err(ErrorKind::UnsupportedFeature("template reference".into()));
            }
        }
    }
    Ok(text)
}

fn parse_host(text: &str) -> Result<Url, ErrorKind> {
    let mut url = Url::parse(text).map_err(|error| ErrorKind::InvalidHost(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ErrorKind::InvalidHost(
            "expected an http(s) URL without credentials, query or fragment".into(),
        ));
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

fn request_url(base: &Url, path: &str) -> Result<Url, ErrorKind> {
    if !path.starts_with('/') || path.starts_with("//") {
        return Err(ErrorKind::InvalidPath(
            "expected a path beginning with one '/'".into(),
        ));
    }
    validate_path_encoding(path)?;
    // A leading `./` forces a relative URL reference: `/foo:bar` and
    // `/http://host/path` must remain paths, not become new URL schemes.
    let url = base
        .join(&format!(".{path}"))
        .map_err(|error| ErrorKind::InvalidPath(error.to_string()))?;
    if url.origin() != base.origin()
        || url.fragment().is_some()
        || !url.path().starts_with(base.path())
    {
        return Err(ErrorKind::InvalidPath(
            "path must stay under the host prefix and must not contain a fragment".into(),
        ));
    }
    Ok(url)
}

fn validate_path_encoding(path: &str) -> Result<(), ErrorKind> {
    let raw_path = path.split_once('?').map_or(path, |(before, _)| before);
    let bytes = raw_path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\' {
            return Err(ErrorKind::InvalidPath(
                "backslashes are not permitted in paths".into(),
            ));
        }
        if bytes[index] == b'%' {
            let decoded = bytes
                .get(index + 1)
                .and_then(|first| hex_digit(*first))
                .zip(bytes.get(index + 2).and_then(|second| hex_digit(*second)))
                .map(|(first, second)| (first << 4) | second)
                .ok_or_else(|| ErrorKind::InvalidPath("invalid percent escape in path".into()))?;
            // Servers can decode separators or dot segments before routing;
            // allowing these would let a request escape an `@host` path prefix.
            if matches!(decoded, b'.' | b'/' | b'\\' | b'%') {
                return Err(ErrorKind::InvalidPath(
                    "encoded traversal or separator in path".into(),
                ));
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    Ok(())
}

fn hex_digit(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

fn encode_body(value: &NovaValue) -> Result<Vec<u8>, ErrorKind> {
    let mut json = String::new();
    encode_value(value, &mut json)?;
    Ok(json.into_bytes())
}

fn encode_value(value: &NovaValue, json: &mut String) -> Result<(), ErrorKind> {
    match value {
        NovaValue::Null => json.push_str("null"),
        NovaValue::Bool(value) => json.push_str(if *value { "true" } else { "false" }),
        NovaValue::Number(value) => {
            if !value.is_finite() || (value.fract() == 0.0 && value.abs() > 9_007_199_254_740_991.0)
            {
                return Err(ErrorKind::InvalidBody(
                    "non-finite or unsafe large integer; JSON numbers currently use f64".into(),
                ));
            }
            if value.fract() == 0.0 {
                json.push_str(&format!("{value:.0}"));
            } else {
                json.push_str(&serde_json::to_string(value).expect("finite f64 can be encoded"));
            }
        }
        NovaValue::String(value) => {
            json.push_str(&serde_json::to_string(value).expect("string can be encoded"))
        }
        NovaValue::Array(values) => {
            json.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    json.push(',');
                }
                encode_value(value, json)?;
            }
            json.push(']');
        }
        NovaValue::Object(members) => {
            json.push('{');
            for (index, (key, value)) in members.iter().enumerate() {
                if index != 0 {
                    json.push(',');
                }
                json.push_str(&serde_json::to_string(key).expect("key can be encoded"));
                json.push(':');
                encode_value(value, json)?;
            }
            json.push('}');
        }
        NovaValue::Ref(_) => return Err(ErrorKind::UnsupportedFeature("body reference".into())),
    }
    Ok(())
}

fn run<T: Transport>(
    requests: Vec<PreparedRequest>,
    transport: &T,
    options: &RunOptions,
) -> Result<RunReport, ExecutionFailure> {
    let mut completed = RunReport::default();
    for request in requests {
        let response: HttpResponse =
            transport
                .send(&request, options)
                .map_err(|kind| ExecutionFailure {
                    offset: request.offset,
                    kind,
                    completed: std::mem::take(&mut completed),
                })?;
        completed.requests.push(ResponseRecord {
            name: request.name,
            method: request.method,
            url: request.url,
            status: response.status,
            headers: response.headers,
            body: response.body,
        });
    }
    Ok(completed)
}

#[cfg(test)]
mod tests;
