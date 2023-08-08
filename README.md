> **⚠️ This project is in development and is not ready for use in production.**

<br>

<img src="assets/logo.png" height="80px" />
<hr>

This is the official repository for the WebX web server project.
A framework for building minimal but powerful backends for web applications using **HTMX** for the frontend. The goal is to reduce the boilerplate while still providing the flexibility to build complex applications.
WebX is a web server written in Rust that is designed to be fast, lightweight, and easy to use.


## Features
- [ ] WebX Language
    - [ ] Routing
    - [ ] JSX support
    - [ ] TypeScript support
    - [ ] ORM
        - [ ] Model definitions
        - [ ] Queries
        - [ ] Migrations
    - [ ] Validation
    - [ ] Handlers
    - [ ] Middleware
    - [ ] Error handling
    - [ ] Templating
    - [ ] Authentication
    - [ ] Authorization
    - [ ] Built-in services
        - [ ] Sessions
        - [ ] Caching
        - [ ] Static file serving
        - [ ] WebSockets
- [X] Blazingly fast 🔥 and lightweight
- [X] Simple, easy to learn and intuitive syntax
- [ ] Batteries included 🔋 (stdlib)
- [ ] Hot reloading
- [ ] Production mode
- [ ] Automatic SSL and HTTPS
- [ ] Database support (PostgreSQL, MySQL, SQLite)
- [ ] DDOS protection 🛡️
- [ ] Input sanitization (safe defaults)
- [ ] Output sanitization (safe defaults)

Do you have any suggestions for additional features? Create an issue or a pull request!

## Installation
Download the latest release with prebuilt binaries from the [releases page](https://github.com/WilliamRagstad/WebX/releases) and extract it to a folder of your choice.

## Usage
To start the web server for a project, use:
```sh
webx run
```

To run the project in production mode, use:
```sh
webx run --prod
```

## Examples
The following is an example of a simple WebX application:
```typescript
model User {
  id: number @primary @autoincrement;
  name: string[maxLength(50))]
  email: string?; // optional
  isAdmin: boolean;
}

model Todo {
  id: number @primary @autoincrement;
  task: string;
  userId: number @foreign(User.id);
}

handler initServices() { 
  userService: new UserService(),
  todoService: new TodoService()
}

handler auth(userService: UserService, id: number) {
  user: userService.getCurrentUser(id)
}

handler isAdmin(user: User) {
  if (!user.isAdmin()) {
    return error("User is not an admin.");
  }
}

handler renderTodos(todos: Todo[], user: User): HTML {
  return (<div>
      <h1>Welcome, {user.name}!</h1>
      <ul>
        {todos.map(todo => <li>{todo.task}</li>)}
      </ul>
    </div>);
}

// Endpoint: GET /todos/<user id>
get /todos/(id: number) -> initServices(), auth($.userService, id), isAdmin($.user) {
    const todos = $.todoService.getAllTodosForUser($.user.id);
    return { todos };
} -> renderTodos($.todos, $.user)
```

