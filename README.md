# Nova

> An open-source DSL for writing integrated HTTP tests, plus a CLI and TUI for running quick requests with minimal friction.

Nova lets developers describe HTTP requests and assertions in plain `.nova` files, commit them to the repository, and run them as integration tests. The same files, and the same language, power a CLI and a TUI built for firing off HTTP queries quickly.

> **Status:** early development. The language parser is implemented. The execution engine, CLI and TUI are not yet. See the [Roadmap](#roadmap).

---

## What is Nova?

Nova has two goals, both built on the same language.

1. **Integrated tests as code.** Push `.nova` files to your repository and use them to make HTTP test assertions: chain requests, capture values from responses, and assert on the results. Tests are plain text, reviewable in pull requests, and run in CI.
2. **Quick HTTP requests.** Write a request and run it, with as little ceremony as possible, from the CLI or from an interactive TUI focused on writing HTTP queries fast.

---

## Why Nova?

Existing tools tend to fall into two camps:

* **GUI-first clients** like Postman and Insomnia are heavy, workspace-oriented, and keep requests out of your repository's natural workflow.
* **CLI tools** like `curl` or `httpie` are lightweight, but are awkward for multi-step flows and have no built-in notion of assertions.

Nova aims to be small enough for ad-hoc requests and expressive enough for repeatable integration tests.

* Tests that live in your repo as plain text
* Minimal, readable syntax
* Request chaining: use a response value in the next request
* Built-in assertions
* Fast startup, terminal-native
* Git-friendly, no hidden state

---

## Philosophy

### Plain text first

No hidden databases, no proprietary formats, no vendor lock-in. A `.nova` file can be committed to Git, reviewed, shared, and edited in any editor.

### Tests and exploration share one language

A request you wrote while poking at an API is already a valid test once you add an assertion beneath it.

### Simplicity over configuration

No workspaces, no collections, no mandatory environments. Write a request and run it.

### Progressive complexity

Simple requests stay simple. Variables, request dependencies, environment values, commands and assertions are added only when you need them.

---

# Language reference

A `.nova` file is a sequence of statements, parsed in source order. Blank lines are insignificant.

## Comments

Comments start with `//` and run to the end of the line. A `//` only starts a comment at the start of a line or after whitespace, so URLs are unaffected.

```nova
// A standalone comment
GET /me // a trailing comment
GET /a//b // "/a//b" is the path, the rest is a comment
```

`#` is not a comment marker; it is a sigil (see [References](#references)).

## Host

`@host` sets the base URL for the requests that follow it.

```nova
@host
http://localhost:3000
```

The value may contain references, e.g. `@env.BASE_URL`.

## Headers

`@header` sets default headers for the requests that follow it. Each line is `Name: Value`. The block continues until a line that is not a header.

```nova
@header
Content-Type: application/json
Authorization: Bearer @accessToken
```

`@host` and `@header` are positional: they apply to every request below them until overridden by a later one.

## Requests

A request is an HTTP method and a path. Supported methods (uppercase only): `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.

```nova
GET /users
```

A request may be preceded by a **label** on its own line, so it can be referenced by assertions and later requests:

```nova
@me
GET /me
```

Reserved names that cannot be used as labels: `host`, `header`, `assert`, `env`.

### Bodies

A body is a JSON-like object or array placed after the request line. It differs from strict JSON in two ways:

* values may be [references](#references)
* trailing commas are allowed

Comments are also allowed inside bodies.

```nova
@login
POST /login
{
    "email": @env.EMAIL,
    "password": "12345678", // trailing comma is fine
}
```

References are bare: `"email": @env.EMAIL` is a reference, whereas `"email": "@env.EMAIL"` is the literal string.

## References

A reference is a sigil followed by dot-separated names.

| Form                              | Meaning (resolved by the execution engine)   |
| --------------------------------- | -------------------------------------------- |
| `@env.EMAIL`                      | A value from the environment                 |
| `@accessToken`                    | A variable defined with an assignment        |
| `@login.response.body.accessToken`| A value from a named request's response      |
| `#auth.email`                     | A parameter of a command                     |

Parsing only checks that a reference is well-formed. Whether it points to something is decided at execution time.

## Variables

Assign a response value (or any reference) to a name with `@name = reference`:

```nova
@accessToken = @login.response.body.accessToken

@header
Authorization: Bearer @accessToken

GET /me
```

## Assertions

Assertions attach to a labelled request with `@assert.<request>.<mode>`.

| Mode         | Form                                | Checks                                      |
| ------------ | ----------------------------------- | ------------------------------------------- |
| `typeOnly`   | block of `field: type`              | Fields exist with the given type            |
| `hasField`   | inline `[ "a", "b" ]`               | The listed fields are present               |
| `exactFields`| inline `[ "a", "b" ]`               | The response has exactly these fields       |
| `fieldMatch` | block of `field: value`             | The listed fields have the given values     |
| `exactMatch` | block of `field: value`             | The response matches exactly                |

Types for `typeOnly`: `string`, `number`, `boolean`, `array`, `object`, `null`.

```nova
@assert.login.typeOnly
accessToken: string

@assert.me.hasField [ "email" ]

@assert.me.exactFields [ "email", "user_id" ]

@assert.me.fieldMatch
email: "john@example.com"

@assert.me.exactMatch
email: "john@example.com"
user_id: "1"
```

## Commands

A command turns a group of requests into a reusable, parameterised flow. Declare it with `#command`, tag requests with `@name.command`, and read parameters with `#command.param`.

```nova
#command auth email password

@login.auth
POST /login
{
    "email": #auth.email,
    "password": #auth.password,
}

@me.auth
GET /me
```

Running `nova run auth <email> <password>` executes every request tagged `.auth`, in source order. A label has at most two segments (`@login.auth`).

---

# Full example

```nova
@host
http://localhost:3000

@header
Content-Type: application/json

@login
POST /login
{
    "email": @env.EMAIL,
    "password": @env.PASSWORD
}

@assert.login.typeOnly
accessToken: string

@accessToken = @login.response.body.accessToken

@header
Authorization: Bearer @accessToken

@me
GET /me

@assert.me.hasField [ "email" ]
```

---

# CLI and TUI

> Planned. Not yet implemented.

* **CLI** - run a `.nova` file or a command (`nova run auth <email> <password>`), suitable for CI. Failed assertions produce a non-zero exit code.
* **TUI** - an interactive interface focused on writing and firing HTTP queries quickly: navigate requests, run the one under the cursor, edit before executing, inspect response bodies and headers, browse JSON, and review history.

---

# Project architecture

```
                +------------------+
                |      TUI         |
                +------------------+
                         │
                +------------------+
                |       CLI        |
                +------------------+
                         │
                +------------------+
                | Execution Engine |
                +------------------+
                         │
                +------------------+
                |      Parser      |
                +------------------+
                         │
                +------------------+
                |  Nova Language   |
                +------------------+
```

The parser is a pure syntax layer that produces a flat, ordered AST. The execution engine is responsible for resolving context (`@host`, `@header`), references, and evaluating assertions. See [`docs/superpowers/specs/2026-08-11-nova-parser-design.md`](docs/superpowers/specs/2026-08-11-nova-parser-design.md) for the parser design.

---

# Roadmap

* [x] Nova language parser
* [ ] HTTP execution engine
* [ ] Assertion runner and test reporting
* [ ] CLI (`nova run`)
* [ ] Interactive TUI
* [ ] Environment variables (`@env`)
* [ ] Request dependencies and variables
* [ ] Syntax highlighting
* [ ] Cookie management
* [ ] Request history
* [ ] Formatting (`nova fmt`)
* [ ] Linting (`nova lint`)
* [ ] Language Server Protocol (LSP)
* [ ] VS Code extension
* [ ] Neovim support

---

# Open Source

Nova is an open-source project. Contributions, ideas, discussions, and feedback are welcome. Open an issue or start a discussion.

# License

Nova is licensed under the MIT License.
