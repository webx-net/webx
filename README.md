<br>
<div align="center">
  <img src="assets/logo_grad.png" width="200px">

![GitHub Workflow Status (with event)](https://img.shields.io/github/actions/workflow/status/webx-net/webx/rust.yml)
![GitHub all releases](https://img.shields.io/github/downloads/webx-net/webx/total)
![GitHub repo size](https://img.shields.io/github/repo-size/webx-net/webx) \
![GitHub commit activity (branch)](https://img.shields.io/github/commit-activity/w/webx-net/webx)
![GitHub contributors](https://img.shields.io/github/contributors/webx-net/webx)
[![Discord](https://img.shields.io/discord/1163101679760179220?label=Discord)](https://discord.gg/WBYrWf6u7x)
![GitHub Sponsors](https://img.shields.io/github/sponsors/webx-net)

</div>
<br>

This is the official repository for the WebX web server project.
A framework for building minimal but powerful **web app backends**, **REST APIs**, and **hypermedia systems**.\
▸ [Get started](#getting-started) or [read more](#why-webx) about the project below.

> **⚠️ WebX is still in early development and not ready for production use.**


## Features & Roadmap
Below is a high-level overview of the existing (implemented) and planned features of the WebX technology (in no particular order).

- [X] Blazingly fast 🔥
- [X] Lightweight and minimalistic 🪶
- [X] Versatile, flexible, and powerful 🤸
- [X] Easy to use and learn 🧠

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
  - [ ] Model definitions *(ORM, Prism-like)*
    - [X] Fields
    - [X] Types
    - [ ] Constraints
    - [ ] Relations
    - [ ] Migrations
    - [ ] Queries *(CRUD operations)*
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
### Installation
Download the latest prebuilt binaries from the [releases page](https://github.com/WilliamRagstad/WebX/releases) and extract it to a folder of your choice.

### Usage
To start the web server in `dev` mode for a project, use:
```sh
webx run
```
This enables **hot reloading** 🔥, **extensive logging**, **safe real-time error reporting**, and other useful features.
<br><br>
To run the project in production mode, use:
```sh
webx run --prod
```
This enables **optimizations** 🚀 (*for speed and memory performance improvements*), **caching**, **compression**, and other valuable features. It also *disables hot reloading and logging* to **not leak any sensitive information**.

<br>

## Examples
The following is an example of a simple WebX to-do list application.
```typescript
include "../common.webx"

global {
  const db = new Database("sqlite://db.sqlite");
}

model User {
  id: number @primary @autoincrement;
  name: string[maxLength(50))]
  email: string?;
  isAdmin: boolean;
}

model Todo {
  id: number @primary @autoincrement;
  task: string;
  userId: number @foreign(User.id);
}

handler initServices() {
  return {
    userService: new UserService(db),
    todoService: new TodoService(db)
  };
}

handler auth(userService: UserService, id: number) {
  const user = userService.getCurrentUser(id);
  if (!user) error("User not found.");
  return { user };
}

handler isAdmin(user: User) {
  if (!user.isAdmin()) error("User is not an admin.");
}

handler renderTodos(todos: Todo[], user: User): HTML {
  return (<div>
      <h1>Welcome, {user.name}!</h1>
      <ul>
        {todos.map(todo => <li>{todo.task}</li>)}
      </ul>
    </div>);
}

get /todos/(id: number) -> initServices:s, auth(s.userService, id):a, isAdmin(a.user) {
    const todos = s.todoService.getAllTodosForUser(a.user.id);
} -> renderTodos(todos, a.user)
```

<br>

## Why <b>Web<font color="#3d72d7">X</font></b>?
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
### What about <b><font color="#3d72d7">///_h</font>yper<font color="#3d72d7">s</font>cript</b>?
HyperScript is a front-end JavaScript DSL for creating and manipulating HTML DOM elements. It is lightweight, tightly coupled with your HTML code, and is easy to learn.\
*While JSX is the default JS+HTML DSL* in Webx, HyperScript is supported **natively** by WebX and can also be configured to be used in the project settings.\
▸ [Read more about HyperScript](https://hyperscript.org/)

<br>

## Contributing
Contributions are welcome!
If you have any suggestions or find any bugs, please create an issue or a pull request.

Do you have any suggestions for additional features?
Create an issue or a pull request!
[Read more](#contributing) about contributing below.
