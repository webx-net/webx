# Todo CRUD example with HTMX

From the repository root, run:

```sh
cargo run --locked -- run examples/todo
```

Open http://localhost:8080 to add, list, edit, complete, and delete todos. The
page loads pinned **HTMX 2.0.8** from jsDelivr with an integrity hash, so the
browser needs network access to that CDN. No database or frontend build step is
required.

HTMX owns all client/server interaction: `hx-get` loads the list and edit form,
`hx-post` creates a todo, `hx-put` saves it, and `hx-delete` removes it. Every
endpoint uses an inline JavaScript `{ ... }` block and returns a server-rendered `#todo-panel` HTML fragment, which HTMX swaps
with `hx-target` and `hx-swap="outerHTML"`. Titles are escaped on the server in
both text and input-value attributes. Validation messages appear in the returned
fragment's status region; the browser does not render JSON or construct list rows.

CRUD logic lives directly in the `get`, `post`, `put`, and `delete` endpoint
blocks. `global` holds only the shared todo state and reusable HTML rendering
and escaping helpers. Endpoint locals are isolated per request; editing an
endpoint block preserves the shared state.

PUT uses the documented WebX form declaration directly:

```webx
put /todos/(id: String) form(title: String, completed: Boolean) {
    // title and completed are decoded, request-local values.
}
```

The edit form has `hx-put="/todos/1"` and named `title`/`completed` inputs.
[HTMX submits the form inputs](https://htmx.org/docs/#parameters) as an
`application/x-www-form-urlencoded` request body. No PUT URL rewriting or
`hx-params="none"` is needed. An unchecked completion checkbox is omitted by the
browser and binds as `false`; the checked value `true` binds as `true`.
The POST create form retains its URL-based route and small request hook.
There is no custom fetch/XHR code.

Try the complete HTTP lifecycle (responses are HTML):

```sh
curl http://localhost:8080/todos
curl -X POST http://localhost:8080/todos/Buy%20milk
curl http://localhost:8080/todos/1
curl -X PUT http://localhost:8080/todos/1 \
  --data-urlencode 'title=Buy oat milk' --data 'completed=true'
# Omit completed (or send completed=false) to mark it incomplete:
curl -X PUT http://localhost:8080/todos/1 --data-urlencode 'title=Buy oat milk'
curl -X DELETE http://localhost:8080/todos/1
curl http://localhost:8080/todos
```

`GET /todos/1` renders the list with todo 1's edit form open. PUT receives the
already-decoded title from the body; POST decodes its title URL parameter in the
endpoint. Both trim titles and require 1–120 characters. Missing IDs and invalid
title lengths return HTML validation messages (HTTP 200) for HTMX to swap, without
changing stored data. Malformed form bodies and invalid boolean values are
rejected by the runtime with 400; unsupported media types return 415.

This is a small in-memory demo. Creation titles still appear in POST URLs;
PUT titles travel in the body. Bodies are limited to 64 KiB, and multipart/file
uploads are unsupported.
Restarting, or editing the module's global block, resets all data; HTML-only
edits preserve it. There is no persistence, authentication, or user separation.
See the [runtime contract](../../docs/runtime.md) for supported facilities and
limitations.

`cargo test --locked todo_example` checks the real example over HTTP, including
the CRUD lifecycle, HTMX fragment controls, validation, and escaped titles.
