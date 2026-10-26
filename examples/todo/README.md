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
endpoint returns a server-rendered `#todo-panel` HTML fragment, which HTMX swaps
with `hx-target` and `hx-swap="outerHTML"`. Titles are escaped on the server in
both text and input-value attributes. Validation messages appear in the returned
fragment's status region; the browser does not render JSON or construct list rows.

A small `htmx:configRequest` hook encodes form values into URL segments because
WebX does not yet decode form/JSON bodies. `hx-params="none"` prevents HTMX from
also submitting those values in the body. There is no custom fetch/XHR code.

Try the complete HTTP lifecycle (responses are HTML):

```sh
curl http://localhost:8080/todos
curl -X POST http://localhost:8080/todos/Buy%20milk
curl http://localhost:8080/todos/1
curl -X PUT http://localhost:8080/todos/1/Buy%20oat%20milk/true
curl -X DELETE http://localhost:8080/todos/1
curl http://localhost:8080/todos
```

`GET /todos/1` renders the list with todo 1's edit form open. Titles are decoded
with `decodeURIComponent`, trimmed, and restricted to 1–120 characters. IDs are
generated monotonically; completion values are `true` or `false`. Missing IDs and
invalid inputs return an HTML validation message without changing stored data.
These responses use HTTP 200 so HTMX swaps the message into the page; custom
application status codes are not supported yet.

This is a small in-memory demo. Titles appear in URLs and request logs.
Restarting, or editing the module's global block, resets all data; HTML-only
edits preserve it. There is no persistence, authentication, or user separation.
See the [runtime contract](../../docs/runtime.md) for supported facilities and
limitations.

`cargo test --locked todo_example` checks the real example over HTTP, including
the CRUD lifecycle, HTMX fragment controls, validation, and escaped titles.
