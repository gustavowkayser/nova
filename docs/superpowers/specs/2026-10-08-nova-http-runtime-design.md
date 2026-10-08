# Nova HTTP Runtime — Design Proposal

Date: 2026-10-08
Status: Approved for implementation (scope and host semantics confirmed 2026-10-08)

## Goal and boundary

Execute the already-parsed, ordered `parser::nova::Document` as HTTP requests and return their responses to future CLI/TUI callers. The first increment covers literal requests, positional host and headers, JSON request bodies, and raw HTTP responses. It does not yet implement the rest of the execution engine (references, assertions, commands or reporting of test outcomes).

The parser continues to own syntax only. The runtime does not reparse `.nova` source or modify its AST. It folds statements in source order and owns HTTP effects and runtime errors.

## Is `src/parser/json_parser.rs` in the right place?

**Yes, for now.** It is a strict JSON *parser*, built on `src/parser/parser.rs`; `src/parser/nova/value.rs` already reuses its `keyword`, `string_literal`, and `number` parsers. Nova bodies intentionally use a different `NovaValue` grammar because they allow references, comments and trailing commas. The strict `parse_json` and `JsonValue` can remain available for future response interpretation; parsing arbitrary HTTP response bodies eagerly would be wrong (they need not be JSON). The runtime belongs in `src/runtime/`, **not** under `src/parser/`.

What feels odd is largely naming and ownership: `parser/parser.rs` is the combinator library, `parser/json_parser.rs` is both a grammar and a JSON value type, and `parser/nova/value.rs` has another value type. Renaming the first two to `combinator.rs` / `json.rs`, or extracting a shared JSON value type, would be a separate cleanup, not a prerequisite. Avoid churn to the tested parser during the first HTTP increment. If/when response JSON references are implemented, revisit one shared JSON representation; the current `f64` numbers cannot round-trip large JSON integers exactly.

## Scope of this increment

**Included**

- Run all ordinary requests in document/source order, synchronously, with no CLI or TUI required.
- `@host` sets a base URL (including an optional path prefix) for subsequent requests; each later `@host` replaces it.
- `@header` replaces the current default header set for subsequent requests; headers are case-insensitive by name. A request body receives `Content-Type: application/json` if no content type is set.
- Map all seven AST methods (`GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`) to HTTP, serialize literal `NovaValue` object/array bodies to strict JSON, and return status, response headers and raw response bytes (including non-JSON or non-UTF-8 bodies).
- Treat any HTTP response status, including 3xx/4xx/5xx, as a response, not a transport error. Do not follow redirects in this increment, so a 3xx is observable.
- Surface actionable errors with the originating `Statement.offset` and the responses completed before a transport failure.

**Explicitly deferred**

- Resolution/interpolation of `TemplatePart::Ref`, `NovaValue::Ref`, assignments, `@env`, `#command` parameters and named response references.
- Assertion evaluation, test pass/fail reporting, `nova run`, CLI/TUI, cookie jar, history, streaming, and response JSON traversal.
- No silent omission: if the document contains an unsupported reference, assignment, assertion, command declaration, or command-tagged request, return an `UnsupportedFeature` error **before sending any HTTP requests**. Plain request labels remain valid identifiers in the result.

These omissions align with the separate roadmap entries for assertions, CLI, environment variables, and request dependencies in `README.md`. This is a deliberately useful HTTP slice, not a claim that the full execution engine is complete.

## Proposed interfaces and layout

Add `src/lib.rs` to expose the existing parser and new runtime to future CLI/TUI callers; move the module declaration out of `src/main.rs` so there is only one compilation of parser tests. Keep `main` a placeholder; do not add CLI behavior in this increment.

```
src/lib.rs                 public parser + runtime modules
src/runtime/mod.rs         document walk, positional state, validation, results/errors
src/runtime/http.rs        HTTP client adapter; injectable transport for offline tests
src/parser/…               unchanged
```

Public entry point, in concept (exact Rust types can be refined at implementation time): `execute(document: &Document, options: &RunOptions) -> Result<RunReport, ExecutionFailure>`. `RunOptions` holds a request timeout (default 30 s) and maximum response size (default 10 MiB). `RunReport` contains one ordered record per executed request: optional label, method, final URL, status, response headers, raw response bytes. `ExecutionFailure` contains the statement offset, a specific error kind (unsupported feature, missing/invalid host, invalid path/header/body, transport/timeout, oversized response), and any already-completed records. No secrets or request bodies are printed implicitly.

Use a synchronous public API backed by `reqwest` with Rustls TLS, with an internal Tokio deadline covering both sending and reading the whole response; use `serde_json` for strict outbound JSON encoding and a URL parser for joining and validation, not string concatenation. Keep the transport behind a small private interface so tests can inject a fake without opening sockets. New dependencies are runtime-only; do not replace the existing parser with `serde_json` as part of this change.

## Execution rules

1. Preflight the document for unsupported constructs and validate known literal configuration before any network I/O; do not partially execute a file simply because a later statement uses an unimplemented feature.
2. Fold the ordered statements: `Host` updates the base URL; `Headers` replaces the header set; each `Request` captures the current configuration and is sent sequentially. Documents with no requests yield an empty report.
3. Require `@host` before the first request. Accept an absolute `http://` or `https://` base URL with an optional path prefix (no query, fragment or credentials); accept request paths beginning with exactly one `/`, optionally with a query. Append the request path *under* the host path: `@host http://example.com/api` + `GET /users` targets `http://example.com/api/users`. Normalize a missing trailing slash on the base before URL joining, join as a relative URL path (never re-interpret `/foo:bar` as a scheme), reject network-path forms (`//other-host/...`), dot-segment traversal that escapes the prefix, and encoded separators/dot segments that an upstream might decode into traversal. Ensure the origin does not change.
4. Validate header names/values with the HTTP client; when the same name occurs more than once in a block, the last value wins. Replace the entire block at the next `@header`. Only add the default JSON content type when sending a body and none is provided.
5. Convert a literal body recursively into JSON for transmission. Quoted `"@env.X"` is a string, not a reference. Serialize safely representable integral values as JSON integer literals; reject non-finite numbers and integral `f64` values above the maximum safe integer magnitude (2^53 - 1) rather than silently sending potentially rounded large IDs. The current AST stores numbers as `f64`, so exact representation of arbitrary decimal fractions is not guaranteed; lossless JSON numbers require a separate parser/AST change.
6. Bound the entire send-and-response-read by one deadline per request, and each response body by the configured size limit. Keep response bytes as-is; JSON decoding is deferred until some consumer needs it. Never swallow a network/encoding error or reinterpret an HTTP 4xx/5xx as a transport failure.

## Acceptance tests

- Offline transport tests: all seven methods, source-order execution, labels, positional host/header replacement, repeated header names, JSON body encoding (nested arrays/objects, escaped strings), and default versus explicit content type.
- Preflight tests: missing/invalid host, invalid or origin-switching path, invalid headers, unsupported references in host/path/headers/nested body, assignment/assertion/command/tagged request, and no requests sent after any preflight failure.
- Result/error tests: 3xx/4xx/5xx preserved; non-JSON and binary responses retained; timeout/transport/size errors carry statement offset and partial report; no-requests document succeeds.
- At least one local-server integration test for an actual GET and POST (no external network), plus `cargo test` for existing parser regression coverage.

## Decisions recorded

- Implement the literal-only HTTP slice first. Resolve references, `@env`, assertions and commands in later milestones.
- Support path prefixes on `@host`.
- Defer lossless JSON numbers: fixing both parser ASTs and their tests is more than a small HTTP change. Reject unsafe large integral values during encoding and document the remaining fractional precision limitation. Revisit the representation before promising exact JSON round-tripping.
