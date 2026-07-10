# Nova

> A terminal-native HTTP client with a powerful TUI and its own request language.

Nova is an open-source developer tool designed to make working with HTTP APIs fast, intuitive, and enjoyable from the terminal.

Unlike traditional API clients, Nova is built around a simple, human-readable request language and an interactive Terminal User Interface (TUI). The goal is to eliminate unnecessary complexity while providing a modern experience for exploring, testing, and automating HTTP requests.

---

## Why Nova?

Existing HTTP clients generally fall into one of two categories:

* **GUI-first applications** like Postman and Insomnia provide many features but often become heavy and workspace-oriented.
* **CLI tools** like `curl` or `httpie` are lightweight and scriptable but lack an intuitive interface for interactive exploration.

Nova combines the best of both worlds.

* ⚡ Fast startup
* 🖥️ Terminal-native experience
* 📁 Plain text request files
* 🎯 Minimal and intuitive syntax
* 🔗 Reusable variables and request chaining
* 🌐 Git-friendly and versionable
* ❤️ Built for developers

---

## Philosophy

Nova follows a few simple principles.

### Plain text first

Requests should live in plain text files.

No hidden databases.
No proprietary formats.
No vendor lock-in.

A `.nova` file can be committed to Git, reviewed in pull requests, shared with teammates, and edited using any text editor.

### Terminal-native

Nova is designed for developers who spend most of their time in the terminal.

The interface should feel responsive, keyboard-driven, and fast.

### Simplicity over configuration

Creating and executing a request should require as little friction as possible.

No workspaces.

No collections.

No mandatory environments.

Just write a request and run it.

### Progressive complexity

Simple requests should stay simple.

As projects grow, Nova should naturally support variables, reusable values, request dependencies, environments, authentication, and more—without sacrificing readability.

---

# Example

```nova
# Base URL
http://localhost:3000

# Global headers
Content-Type: application/json

GET /users

POST /users
{
    "name": "John",
    "email": "john@example.com"
}

http://localhost:8080

GET /health
```

Each request inherits the current context (base URL, headers, variables, etc.) unless explicitly overridden.

---

# Variables

Nova allows values to be reused throughout the file.

Variables may come from literals, request responses, or future language features.

```nova
@host
http://localhost:3000

@get-user
GET /users/42

@user-id
@get-user.id

POST /orders
Authorization: Bearer @token
{
    "userId": @user-id
}
```

---

# Interactive execution

Nova is not just a parser.

It is an interactive execution environment.

From the TUI you can:

* Navigate requests
* Execute the request under the cursor
* Edit parameters before execution
* Inspect responses
* View headers
* Browse JSON responses
* Copy values
* Review execution history

The goal is to make API exploration feel as fluid as browsing source code.

---

# Project Architecture

Nova is divided into independent components.

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

This separation allows multiple interfaces to share the same execution engine and request language.

---

# Roadmap

The project is currently in its early stages.

Planned features include:

* [ ] Nova language parser
* [ ] HTTP execution engine
* [ ] Interactive TUI
* [ ] Syntax highlighting
* [ ] Variables
* [ ] Request dependencies
* [ ] Environment variables
* [ ] Authentication helpers
* [ ] Cookie management
* [ ] Request history
* [ ] Collections
* [ ] Formatting (`nova fmt`)
* [ ] Linting (`nova lint`)
* [ ] Language Server Protocol (LSP)
* [ ] VS Code extension
* [ ] Neovim support

---

# Open Source

Nova is an open-source project built for developers who love the terminal.

Contributions, ideas, discussions, and feedback are always welcome.

If you have suggestions or would like to help shape the future of the project, feel free to open an issue or start a discussion.

---

# License

Nova is licensed under the MIT License.
