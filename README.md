<br>
<div align="center">
  <img src="assets/webx.png" alt="Web X" height="90px">
<br><br>

![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/webx-net/webx/rust.yml)
![GitHub all releases](https://img.shields.io/github/downloads/webx-net/webx/total)
![GitHub repo size](https://img.shields.io/github/repo-size/webx-net/webx) \
[![Discord](https://img.shields.io/discord/1163101679760179220?label=Discord)](https://discord.gg/WBYrWf6u7x)
![GitHub contributors](https://img.shields.io/github/contributors/webx-net/webx)
![GitHub Sponsors](https://img.shields.io/github/sponsors/webx-net)

</div>
<br>

This is the official repository for the WebX web server project.
A framework for building minimal but powerful **web app backends**, **REST APIs**, and **hypermedia systems** using a [HTML first](https://html-first.com/) approach.\
▸ [Get started](#getting-started) or [read more](#why-webx) about the project below.

> **⚠️ WebX is still in early development and not ready for production use.**
> See [current runtime support and limitations](docs/runtime.md).

<br>

## Features & Roadmap

Below is a high-level overview of the existing (implemented) and planned features of the WebX technology (in no particular order).

- [X] Blazingly fast[^1] 🔥
- [X] Lightweight and minimalistic 🪶
- [X] Versatile, flexible, and powerful 🤸
- [X] Easy to use and learn 🧠

[^1]: [BLAZING FAST @ThePrimeagen](https://www.youtube.com/watch?v=1M9hPXg-bFM)

<details><summary>Batteries included 🔋</summary>

- [X] Hot reloading 🔥
- [ ] Protected and secure by default 🔒
  - [ ] DDOS 🛡️
  - [ ] CORS 🛡️
  - [ ] CSRF 🛡️
  - [ ] XSS 🛡️
  - [ ] SQL injection 🛡️
  - [ ] Rate limiting ⏱️
  - [ ] TLS/SSL/HTTPS 🔒
- [ ] Built-in modules and services 📦
  - [X] Static file serving 📁
  - [ ] Database driver integration 🗃️
    - [ ] PostgreSQL
    - [ ] MySQL
    - [ ] SQLite
    - [ ] MariaDB
    - [ ] MongoDB
    - [ ] Redis
  - [ ] Authentication 🔑
  - [ ] Authorization 🔑
  - [ ] Caching 🗄️
  - [ ] Sessions 🍪
  - [ ] WebSockets 🌐
- [X] [VSC extension](https://github.com/webx-net/webx-extension) *(Syntax Highlighting, Snippets, and more)*
- [ ] WebX Playground 🛝
- [ ] WebX Docs 📖
- [ ] Package manager 📦 *(`NPM`-like, for WebX handlers, modules, and drivers)*

</details>

<details><summary>WebX DSL (Domain Specific Language)</summary>

  - [X] Parser
    - [X] AST for WebX modules
  - [X] Comments
  - [ ] Model definitions ~~*(ORM, Prism-like)*~~
    - [X] Fields
    - [X] Types
    - [ ] Value Constraints
    - [ ] Serde Validation
    - [ ] ~~Relations~~
    - [ ] ~~Migrations~~
    - [ ] ~~Queries *(CRUD operations)*~~
  - [ ] Native SSR *(templating)*
    - [X] JSX support
    - [ ] ~~HyperScript support~~
  - [ ] TypeScript support
    - [X] Deno runtime
    - [ ] ~~WebAssembly AoT compilation~~
    - [ ] Unified type definitions *(shared between client and server)*
  - [ ] Validation *(Serialize/Deserialize for req/res data)*
    - [ ] Input sanitization *(safe defaults)*
    - [ ] Output sanitization *(safe defaults)*
    - [ ] Type checking
    - [ ] Constraints
    - [ ] Relations
    - [ ] Formats
      - [ ] JSON
      - [ ] JSON5
      - [ ] XML
      - [ ] HTML
      - [ ] CSV
      - [ ] YAML
      - [ ] TOML
      - [ ] Custom *(plugins)*
  - [ ] Route definitions
    - [X] HTTP methods
    - [X] Path parameters *(URL/segments/)*
    - [ ] Query parameters *(`?key=value`)*
    - [ ] Request headers
    - [ ] Request Body parameters *(POST/PUT/PATCH)*
    - [ ] Body serialization *(JSON, XML, etc.)*
    - [ ] Body deserialization and validation
    - [ ] Dependency injection *(between handlers)*
    - [ ] Middleware *(before/after handlers)*
    - [ ] Endpoint body code block types
      - [ ] TypeScript *(TS)*
      - [ ] Typescript-React *(TSX)*


```typescript
get /todos/(id: number) -> initServices:s, auth(s.userService, id):user, isAdmin(user) { ... }
```
  - [ ] Handlers *(middleware)*
    - [ ] Design choices
      - [ ] Async vs sync
      - [ ] Return types *(explicit)*
      - [ ] Error handling *(opinionated!)*
      - [ ] Dependency injection
      - [ ] Return result destructuring
  - [ ] Built-in handlers 📦 *(stdlib)*
    - [ ] Data manipulation
    - [ ] Business logic
    - [ ] Authentication
      - [ ] OAuth
      - [ ] OpenID
      - [ ] JWT
      - [ ] SAML
      - [ ] LDAP
      - [ ] Kerberos
      - [ ] Basic
      - [ ] Digest
      - [ ] Bearer
      - [ ] API keys
      - [ ] HMAC
      - [ ] Mutual TLS
    - [ ] Authorization
      - [ ] RBAC
      - [ ] ABAC
      - [ ] PBAC
      - [ ] LBAC
      - [ ] DAC
    - [ ] Logging
      - [ ] Request logging
      - [ ] Error logging
      - [ ] Audit logging
      - [ ] Security logging
      - [ ] Performance logging
      - [ ] Debug logging
      - [ ] Custom logging *(plugins)*
    - [ ] Caching
      - [ ] In-memory
      - [ ] Redis
      - [ ] Memcached
    - [ ] Sessions
    - [ ] Database drivers
    - [X] Static file serving
    - [ ] Templating 📄
- [X] Error handling 🚨
    - [X] Server errors
    - [X] Client errors
    - [X] Network errors
    - [ ] Database errors
    - [ ] External service errors
    - [X] Logging

</details>
<details><summary>WebX CLI tool</summary>

  - [ ] Project
    - [X] Scaffolding *(init new project)*
    - [ ] Configuration
  - [ ] Build *(AoT compilation)*
    - [ ] Static files
    - [ ] TypeScript to WebAssembly
  - [ ] Run *(JIT compilation)*
    - [ ] Development mode
      - [X] Hot reloading
      - [X] Logging
      - [X] Error handling
      - [ ] Debugging
    - [ ] Production mode
      - [ ] Optimizations
      - [ ] Caching
      - [ ] Compression
      - [ ] Security
      - [ ] Logging
      - [ ] Error handling
  - [ ] Test 🧪
    - [ ] Unit tests
    - [ ] Integration tests
    - [ ] End-to-end tests
  - [ ] Deploy ☁️
    - [ ] Docker
    - [ ] Kubernetes
    - [ ] Cloud
  - [ ] Documentation *(auto-generated)*
  - [ ] Publish *(to package registry)*
  - [ ] Versioning
  - [ ] Linting
  - [ ] Formatting

</details>
<details><summary>WebX Runtime</summary>

  - [ ] Web server
    - [X] TCP/IP
    - [X] HTTP Request parsing
    - [X] HTTP Response serialization
    - [X] HTTP Request routing
    - [X] HTTP Request handling
    - [ ] Protocols
      - [X] HTTP/0.9
      - [X] HTTP/1.0
      - [X] HTTP/1.1
      - [ ] HTTP/2
      - [ ] HTTP/3
      - [ ] HTTP/3 server push
      - [ ] TLS/SSL/HTTPS
    - [ ] Multiplexing
    - [ ] Compression
    - [ ] Status codes
    - [ ] Cookies
    - [ ] Caching
    - [ ] Sessions
    - [ ] Multi-threading
    - [ ] Middleware
    - [X] Logging
    - [X] Error handling
  - [ ] Web framework
    - [X] REST API
    - [ ] ~~GraphQL API~~
    - [ ] Hypermedia API
    - [ ] WebSockets API

</details>

<br>

## Getting started

### →&nbsp; Read the [Documentation](https://docs.webx.sh/)

### →&nbsp; Installation

Download the latest prebuilt binaries from the [releases page](https://github.com/WilliamRagstad/WebX/releases) and extract it to a folder of your choice.

### →&nbsp; Development

Run the web server in `dev` mode for the current project:

```sh
webx run
```

This enables **hot reloading** 🔥, **extensive logging**, **safe real-time error reporting**, and other useful features.

### →&nbsp; Production

To run the project in production mode:

```sh
webx run --prod
```

This performs compile-time **optimizations** 🚀 (*for performance improvements*), **caching**, and other valuable features.
It also disables hot reloading and response-logging to reduce sensitive debug information.

<br>

## Examples

The [todo CRUD example](examples/todo/README.md) is a runnable HTMX browser UI
with an in-memory backend, covered by HTTP integration tests. From this repository:

```sh
cargo run --locked -- run examples/todo
```

Open <http://localhost:8080>. HTMX sends CRUD requests and swaps server-rendered
HTML fragments. The example uses `global` JavaScript functions, path parameters,
and `static("index.html")`; it loads pinned HTMX 2.0.8 from jsDelivr. No database,
frontend build step, request-body decoding, or JSX rendering is required.

See [Runtime behavior](docs/runtime.md) for the implemented routing, reload,
request-state, and body-handling contract. The roadmap above includes planned
features; it is not a statement of current runtime support.

Run the tests with `cargo test --locked`. They assert actual HTTP responses and
join the server/runtime workers without terminating the test process.

<br>

## Why <b>Web <font color="#3d72d7">X</font></b>?

**Our vision** is to reduce the boilerplate and complexity of building backends and APIs.\
▸ WebX is designed to be **simple**, **minimal**, **easy to learn**, and **intuitive** while still being **versatile** and **flexible**.
It can build complex applications **quickly** while still being **lightweight🪶** and **blazingly fast🔥**.

> **Backends shouldn't be bloated and complex**, but focus on what's most important.
> Don't reinvent the wheel for every new project,\
> ▸ Jump straight to ***the goodi-goodi parts*** and get [started using](#getting-started) **WebX** today!

### ❤️ Perfect match for <b><<font color="#3d72d7">/</font>> htm<font color="#3d72d7">x</font></b>

WebX is designed to be a minimalistic web framework that is easy to learn and use.
It is ***intended*** to be used with **HTMX**, a great alternative to frameworks like React, Vue, and Angular (or other stacks).
HTMX allows you to build dynamic web applications without learning a new language or framework for the front end.
WebX is designed to be versatile and flexible, and it is easy to quickly build backends for complex applications.\
▸ [Read more about HTMX](https://htmx.org/)

<!--
### What about <b><font color="#3d72d7">///_h</font>yper<font color="#3d72d7">s</font>cript</b>?

HyperScript is a front-end JavaScript DSL for creating and manipulating HTML DOM elements. It is lightweight, tightly coupled with your HTML code, and is easy to learn.\
*While JSX is the default JS+HTML DSL* in Webx, HyperScript is supported **natively** by WebX and can also be configured to be used in the project settings.\
▸ [Read more about HyperScript](https://hyperscript.org/)
-->

<br>

## Contributing

Contributions are welcome!
If you have any suggestions or find any bugs, please create an issue or a pull request.

Do you have any suggestions for additional features?
Create an issue or a pull request!
[Read more](#contributing) about contributing below.
