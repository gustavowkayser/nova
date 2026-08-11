# Nova Language Parser — Design

Date: 2026-08-11
Status: Approved, ready for implementation planning

## Goal

Parse `.nova` source files into an abstract syntax tree. This is the "Parser"
layer of the architecture in `README.md`, sitting directly above the Nova
language and below the execution engine.

## Scope

**In scope.** Syntax only: turning text into a `Document`, and reporting where
and why invalid text failed.

**Out of scope**, deferred to the execution engine:

- Resolving what a reference points at (`@env.EMAIL` vs `@login.response.body.x`)
- Detecting undefined variables, duplicate request names, or references to
  requests that do not exist
- Folding `@host` / `@header` context into individual requests
- Executing requests, evaluating assertions, reading the environment file

"Undefined variable `@token`" is not a parse error. The parser accepts any
syntactically well-formed reference and lets a later pass judge it.

## Foundation

Built on the existing combinator library in `src/parser/parser.rs`
(`Parser<T>`, `then` / `or` / `map` / `many` / `many1` / `sep_by` / `choice` /
`label`, with position-tracking `ParseError` and `best`-error selection).

`parser.rs` is **not modified**. `json_parser.rs` changes only in the
visibility of two functions.

## Decisions

Each of these was chosen deliberately; the rationale matters more than the
choice, because it is what tells you whether a future change is consistent.

### 1. Flat, ordered AST

`parse_nova` returns statements in source order. It does not resolve context.

`@host` and `@header` are positional — they apply to every request below them
until overridden — so something must fold that context in. That something is
the execution engine, not the parser.

Keeping the parser a pure syntax layer matches how `json_parser.rs` is already
written, stops `nova/` from doing two jobs with two different failure modes,
and preserves source order, which is what both `nova fmt` and the planned LSP
need. A resolved dependency graph is the wrong shape for a formatter.

### 2. A distinct `NovaValue`, reusing JSON primitives

Nova request bodies are JSON-*like*, not JSON. They differ in two ways:

- values may be references (`@env.EMAIL`, `#auth.password`)
- trailing commas are permitted

Trailing commas appear in two separate examples in the source specification and
are treated as intentional: they are the friendlier choice for a hand-edited,
diff-reviewed file.

So `NovaValue` is its own type in `nova/value.rs`. `json_parser.rs` remains a
strict RFC 8259 parser with its behaviour and test suite unchanged — it still
rejects trailing commas, because it also parses HTTP *responses*, which are
real JSON.

To avoid two copies of the string-escape and number grammar,
`json_parser::string_literal` and `json_parser::number` become `pub(crate)` and
are called from `value.rs`. Visibility is the only change.

### 3. `@name.command` is a command tag

`@login.auth` means: request named `login`, tagged into command `auth`.

`#command auth email password` declares the command; `nova run auth <email>
<password>` executes every request carrying the `.auth` tag, in source order.
An untagged request has `command: None`.

### 4. Blocks end greedily; blank lines are insignificant

A multi-line block (`@header`, and the line-based assert modes) consumes its
member lines until it reaches something that cannot continue it: EOF, a line
starting with `@` or `#`, or a line that is not well-formed for that block.

Blank lines are pure formatting and carry no grammatical meaning anywhere. A
file does not stop parsing because someone deleted a blank line.

### 5. Line-aware lexical layer, single pass

The combinator library and `json_parser.rs` treat whitespace as insignificant —
`token()` eats `" \t\r\n"` alike. Nova is line-oriented: `Content-Type:
application/json` and `POST /login` are terminated by their newline.

Resolution: two whitespace regimes, each confined to its own layer.

- **Statement level** is line-oriented. `hspace()` matches spaces and tabs and
  never a newline; `eol()` terminates a line.
- **Request bodies** are a free-form island. Inside `{ ... }` newlines are
  irrelevant, because brace matching already delimits the construct.

The two regimes never interact, so neither needs to know about the other.

Rejected alternatives:

- *Pre-split into lines, then parse.* Simpler for `@header`, but bodies span
  many lines, so it forces rejoining a run of lines and re-parsing them —
  reintroducing the problem, and scrambling byte offsets in the process.
- *A separate lexer emitting tokens.* `Parser<T>` is hardwired to `&str`
  (`Rc<dyn for<'a> Fn(&'a str) -> ParseResult<'a, T>>`). Parsing a token stream
  means making `Parser` generic over its input, touching every combinator and
  both existing parsers — a rewrite of tested code to buy what a dozen lines of
  `hspace` / `eol` provide.

### 6. `//` line comments, and the `http://` collision

Comments run from `//` to end of line.

`#` cannot be the comment marker, because this specification uses it as a
sigil (`#command`, `#auth.email`). The `README.md` examples currently use `#`
for comments; the README is stale and needs a follow-up update.

**A comment must be preceded by whitespace or start its line.** Without this
rule, `@host` followed by `http://localhost:3000` parses as `http:` plus a
comment, and every example in the specification breaks.

The rule bites in exactly one place: `literal_run`, the production for template
text, is the only one that greedily consumes arbitrary line content, so it is
the only one that has to decide whether a `//` belongs to it. It stops before
`hspace+ "//"` and keeps everything else. In every other position — after
`@host`, after a closing `]`, on its own line — a `//` is unambiguously a
comment, and plain `eol` handles it. Cases:

| Text                     | `//` preceded by | Result       |
| ------------------------ | ---------------- | ------------ |
| `http://localhost:3000`  | `:`              | part of URL  |
| `GET /a//b`              | `a`              | part of path |
| `GET /me // check this`  | space            | comment      |
| `// standalone`          | start of line    | comment      |

Comments are also recognised inside request bodies — commenting out a field is
exactly when they are wanted — so `value.rs` uses its own whitespace helper
rather than reusing `json_parser`'s.

### 7. Smaller decisions

- **References are structural, not classified.** `Reference` records a sigil
  and its dotted segments. Deciding that `@env.EMAIL` is an environment lookup
  while `@login.response.body.x` is a request path requires a symbol table, and
  belongs to resolution (decision 1).
- **`Template` for host, path and header values.** The same machinery serves
  all three, and `@host` / `@env.BASE_URL` is an obvious near-term need.
- **Byte offset on `Statement` only.** Free to capture, and it lets a runtime
  failure name the offending request. Full spans on every node is the LSP-grade
  version — real work, premature now.
- **Bare references only; no interpolation inside quoted strings.**
  `"email": @env.EMAIL` is a reference; `"email": "@env.EMAIL"` is the literal
  string. Every example uses the bare form. Templates stay in unquoted line
  contexts, where there are no quotes to disambiguate.
- **`Method` is a closed enum**, uppercase only. `get /me` fails with a named
  error rather than quietly working and letting file style drift. Relaxing this
  later is easy; tightening it is not.
- **Bodies are objects or arrays only.** Makes "is there a body?" a
  single-character peek.
- **Labels cap at two segments.** `@login.auth.extra` is rejected by name.

## Module layout

`src/parser/nova_parser.rs` is replaced by `src/parser/nova/`:

| File           | Responsibility                                          | Depends on                          |
| -------------- | ------------------------------------------------------- | ----------------------------------- |
| `ast.rs`       | Type definitions only, no parsing                        | —                                   |
| `lex.rs`       | `hspace`, `comment`, `eol`, `blank_lines`, `ident`, …     | `parser.rs`                         |
| `value.rs`     | `NovaValue` and the body parser                          | `parser.rs`, `json_parser.rs`, `ast.rs` |
| `statement.rs` | The statement grammar                                    | all of the above                    |
| `mod.rs`       | `parse_nova`, `line_column`, re-exports                  | `statement.rs`                      |

Dependencies point one way; there are no cycles. `src/parser.rs` gains
`pub mod nova;`.

The split exists because the whole thing is roughly 800 lines with tests, and
`statement.rs` — the file that will keep changing as the language grows —
should stay readable on its own.

## Entry point

```rust
pub fn parse_nova(input: &str) -> Result<Document, ParseError>
```

Skip leading blank lines, `many1` of `statement`, require EOF — the same shape
as `parse_json`, including the trailing-input check.

```rust
/// Converts a byte offset into a 1-based line and column.
pub fn line_column(input: &str, offset: usize) -> (usize, usize)
```

`ParseError::position` returns a byte offset, which is adequate for JSON but
near-useless to someone editing a line-oriented file.

## AST

```rust
pub struct Document { pub statements: Vec<Statement> }

pub struct Statement {
    pub kind: StatementKind,
    pub offset: usize,           // byte offset of first character
}

pub enum StatementKind {
    Host(Template),              // @host + URL line
    Headers(Vec<Header>),        // @header + Key: Value lines
    Request(Request),
    Assign(Assign),              // @token = @login.response.body.accessToken
    Assert(Assert),
    Command(Command),            // #command auth email password
}

pub struct Request {
    pub name: Option<String>,    // @login
    pub command: Option<String>, // @login.auth -> Some("auth")
    pub method: Method,
    pub path: Template,
    pub body: Option<NovaValue>,
}

pub struct Header  { pub name: String, pub value: Template }
pub struct Assign  { pub name: String, pub value: Reference }
pub struct Assert  { pub request: String, pub assertion: Assertion }
pub struct Command { pub name: String, pub parameters: Vec<String> }

pub enum Assertion {
    TypeOnly(Vec<(String, TypeName)>),    // accessToken: string
    HasField(Vec<String>),                // [ "email" ]
    ExactFields(Vec<String>),             // [ "email", "user_id" ]
    FieldMatch(Vec<(String, NovaValue)>), // email: "john@example.com"
    ExactMatch(Vec<(String, NovaValue)>),
}

/// @env.EMAIL · @accessToken · @login.response.body.accessToken · #auth.email
pub struct Reference { pub sigil: Sigil, pub segments: Vec<String> }
pub enum Sigil { At, Hash }

/// Interleaved literal text and references: `Bearer @accessToken`
pub struct Template { pub parts: Vec<TemplatePart> }
pub enum TemplatePart { Literal(String), Ref(Reference) }

pub enum Method { Get, Post, Put, Patch, Delete, Head, Options }
pub enum TypeName { String, Number, Boolean, Array, Object, Null }

// value.rs
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

`TypeName` covers the six JSON types. The source specification only ever shows
`string`; the rest are inferred and may need revisiting.

## Grammar

```ebnf
document     = blank* , ( statement , blank* )+ , EOF ;
statement    = host | headers | command | assign | assert | request ;

host         = "@host"   , eol , blank* , template , eol ;
headers      = "@header" , eol , blank* , ( header_line , blank* )+ ;
header_line  = header_name , ":" , hspace* , template , eol ;

command      = "#command" , hspace+ , ident , ( hspace+ , ident )* , eol ;

assign       = "@" , ident , hspace* , "=" , hspace* , reference , eol ;

request      = [ label , eol , blank* ] , request_line , [ blank* , body ] ;
label        = "@" , ident , [ "." , ident ] ;
request_line = method , hspace+ , template , eol ;
body         = object | array ;              (* free-form island *)

assert       = "@assert" , "." , ident , "." , mode ;
mode         = "hasField"    , hspace* , string_array , eol
             | "exactFields" , hspace* , string_array , eol
             | "typeOnly"    , eol , blank* , ( type_line  , blank* )+
             | "fieldMatch"  , eol , blank* , ( match_line , blank* )+
             | "exactMatch"  , eol , blank* , ( match_line , blank* )+ ;

type_line    = ident , ":" , hspace* , type_name  , eol ;
match_line   = ident , ":" , hspace* , nova_value , eol ;
type_name    = "string" | "number" | "boolean" | "array" | "object" | "null" ;

method       = "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS" ;

(* --- values: the free-form island, newlines insignificant throughout --- *)
nova_value   = null | bool | number | string | array | object | reference ;
object       = "{" , [ member , { "," , member } , [ "," ] ] , "}" ;
member       = string , ":" , nova_value ;
array        = "[" , [ nova_value , { "," , nova_value } , [ "," ] ] , "]" ;
string_array = "[" , [ string , { "," , string } , [ "," ] ] , "]" ;
(* `string` and `number` are json_parser::string_literal / ::number *)

(* --- lexical --- *)
reference    = ( "@" | "#" ) , ident , ( "." , ident )* ;
template     = ( reference | literal_run )+ ;
literal_run  = ( char - ( "\n" | "@" | "#" ) )+ ;
               (* additionally stops before  hspace+ , "//"  *)

ident        = ( letter | "_" ) , { letter | digit | "_" | "-" } ;
header_name  = ( letter | digit | "_" | "-" )+ ;
hspace       = " " | "\t" ;
comment      = "//" , { char - "\n" } ;
line_break   = [ "\r" ] , "\n" ;
eol          = hspace* , [ comment ] , ( line_break | EOF ) ;
blank        = hspace* , [ comment ] , line_break ;
```

The `blank*` after each block member is what implements decision 4: a blank
line does not end a block, but the first line that cannot be a `header_line`
(or `type_line`, or `match_line`) does. `@login` fails `header_name` at the
`@`; `POST /login` fails at the `:` that isn't there. Both terminate the block
cleanly without lookahead.

### Dispatch

On `@`, read the first segment and branch:

- `host`, `header`, `assert` are reserved and select their block directly
- anything else is a label or an assign, distinguished by scanning past the
  dotted name for an `=`

On `#`, the only statement-level form is `#command`. Everything else must begin
with an HTTP method.

No backtracking beyond a single identifier. `choice`'s `best`-error selection
means a malformed `@header` block reports the failure *inside* the block rather
than a generic "expected a statement".

### Reserved at label position

`host`, `header`, `assert`, `env`. No request may be named `@env`; attempting
it produces a named error rather than a confusing structural one.

### Assert shape

The mode word determines the shape, so no lookahead is needed. `hasField` and
`exactFields` take their string array inline on the same line; `typeOnly`,
`fieldMatch` and `exactMatch` take member lines below.

## Error handling

`ParseError` is reused unchanged. Every statement parser carries a `label` so
failures name a construct — `a header line`, `an HTTP method`, `an assertion
mode` — rather than a character.

Required error cases, each with a message that names the construct:

- unknown or lowercase HTTP method
- label with three or more segments (`@login.auth.extra`)
- unknown assert mode
- `@` with no identifier after it
- unterminated body
- trailing input after the last statement

## Testing

Tests are written before implementation, following the `#[cfg(test)] mod tests`
convention already used in `json_parser.rs`.

- **`lex.rs`** — `//` versus `http://`, `eol` at EOF, comment-only lines,
  `hspace` not consuming newlines.
- **`value.rs`** — references as values, trailing commas, nesting, comments
  inside bodies, and that ordinary JSON parses to the equivalent `NovaValue`.
- **`statement.rs`** — one test per statement form, plus every error case above.
- **`mod.rs`** — **all eight examples from the source specification, verbatim,
  as full-document tests.** These are the acceptance criterion: if all eight
  parse to the expected AST, the parser is complete.

`json_parser.rs`'s existing test suite must continue to pass unmodified, since
this work changes visibility in that file.

## Follow-up, not part of this work

`README.md` documents an older syntax that contradicts this specification: a
bare base URL and bare `Content-Type:` header with no `@host` / `@header`
markers, and `#` used for comments. It should be brought in line in a separate
commit.
