# Runtime behavior

## Routing

Routes are flattened across modules and nested `location` blocks and compiled
into a deterministic table for each HTTP method. Segments are compared from left
to right: literal > named parameter > wildcard. A literal route therefore wins
over a generic route regardless of file discovery or declaration order.

Paths must match all segments. `*` matches exactly one nonempty segment, not a
whole subtree. Trailing and repeated slashes are ignored. Query strings do not
participate in route selection. Parameters are raw URI strings: their declared
types are not checked or converted, and percent escapes are not decoded.

Equivalent routes (including parameters with different names or type annotations)
are rejected with both source locations. Overlapping routes such as
`/fixed/(x: String)` and `/(y: String)/fixed` produce a warning; left-to-right
precedence selects the former for `/fixed/fixed`. Equal-ranked patterns use
lexical path, source filename, and line order as a stable fallback. Methods are
independent; there is no implicit GET-to-HEAD fallback or automatic 405 response.
Missing routes return 404.

## Development reload

Before every development request, WebX checks the `.wx`/`.webx` source tree by
content, as well as responding to filesystem notifications. A completed valid
save is visible on the next request without restarting or sleeping for a watcher.
This includes new files, deletions, and editor atomic-save renames. Production
mode does not reload source modules. Static files are read on each call in either
mode; no browser auto-refresh is provided.

Parsing, route validation, and changed global-block initialization must all
succeed before the new module set and route table become active. A failed edit
reports a diagnostic and retains the complete last good snapshot. HTML/endpoint-code-only
edits preserve module globals. Changing a module's `global` block resets that
module's state; removal discards it. Development checks read the source tree on
each request and are intended for correctness and convenience, not throughput.

## Request state and execution

One actor thread owns one V8 runtime per module. Requests can arrive concurrently
through Hyper, but JavaScript runs synchronously, one request at a time. No V8
handle crosses threads or isolates.

Path parameters, named handler outputs (`-> fn(...): name`), and `out` (the route
body result, available to post-handlers) belong to a single request. Expressions
receive these values as function arguments. They are never installed on the
module's global object and are released after success or failure. Every handler
argument is evaluated once; each named output, including the last in a chain,
is available to subsequent handlers. A failed handler stops the chain.

Module `global` variables are deliberately shared application state, separate
from request bindings. Functions defined there receive request data through their
arguments. Deliberately storing a request value in a global retains it; this is
not a sandbox or per-request VM. Modules have separate globals. Shared objects
returned by handlers remain shared objects, not deep copies. Shared state changes
are serialized but not transactional: a later handler failure does not undo them.

## Supported requests and responses

- Methods are matched exactly (GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS,
  CONNECT, TRACE where supported by the HTTP/1 transport). Bodyless POST/PUT/PATCH
  routes are valid. GET/HEAD/DELETE body declarations are rejected during analysis.
- Route parameters are strings. Request headers and query parameters are not
  exposed as JavaScript bindings. There is no request object or middleware API.
- `form(title: String, completed: Boolean)` declares URL-encoded request fields,
  following the syntax in [the parser documentation](../src/file/parser.rs).
  Send `Content-Type: application/x-www-form-urlencoded` (optionally
  `charset=UTF-8`). Fields are decoded once (`+` means space; `%HH` represents
  UTF-8 bytes) and are available to pre-handlers and endpoint blocks as local
  variables. This works for POST, PUT, and PATCH.
- String fields are required; empty strings are allowed for application validation.
  Boolean/Bool fields accept `true/on/1` and `false/off/0`; an omitted boolean is
  false, matching an unchecked HTML checkbox. Type names are case-insensitive.
  Other field types return 501. Undeclared fields are ignored. Duplicate field
  names, malformed escapes/UTF-8, missing string fields, invalid booleans, and
  collisions with path parameters return 400 before any handler runs. A duplicate
  field declaration is a configuration error (500).
- Missing/wrong Content-Type, non-UTF-8 charsets, and multipart form uploads return
  415 for form routes. `json(...)` and model-body decoding remain unsupported
  (501). On routes without a body declaration, body contents are ignored.
- HTTP tasks collect bodies asynchronously before dispatch, including chunked
  uploads, so a slow upload does not block the JavaScript actor. All request
  bodies are limited to 64 KiB and ten seconds (413/408); read failures return 400.
- Calls to synchronous functions in a module's root `global` block and the native
  `static(path)` handler work. `static` reads a UTF-8 text file relative to the
  project root on every call; it is not a binary asset server or MIME detector.
- Parenthesized route bodies and string handler results are literal HTML
  (`text/html; charset=utf-8`). No JSX interpolation or HTML escaping is performed.
  Other serializable values become JSON (`application/json`); `undefined` becomes
  `null`. Cyclic/nonserializable results and thrown exceptions return 500 without
  stopping the runtime. Successful results use 200; custom response statuses and
  headers are not yet supported.
- Route `{ ... }` blocks execute synchronous JavaScript in a fresh lexical scope
  for each request. Path parameters and pre-handler outputs are available inside;
  `return` supplies the response (or `out` for post-handlers). Without a return,
  the result is `null`. Local declarations do not persist or become post-handler
  bindings; explicitly return values needed by post-handlers. Global application
  state is accessible and remains shared. Syntax/runtime errors return 500 and
  stop the handler chain. Promise/async results still return 501.
- TypeScript transpilation, `.wx` `handler` declarations, model validation,
  includes/import execution, and nested
  location globals/handlers are not implemented. Nested locations do support
  routing with the module's root global functions.

## Limits

The actor remains a throughput bottleneck: slow or nonterminating JavaScript
blocks all requests and can prevent shutdown. There is no worker pool, execution
budget, asynchronous event-loop dispatch, or durable database. In-memory state
is lost on restart or a changed global block. The concurrency tests prove request
binding isolation under simultaneous HTTP traffic, not parallel JavaScript
execution or a production throughput guarantee.

The CLI currently binds loopback port 8080 in development and tries 80/443 in
production (plain HTTP, not TLS); configured host/port settings are not yet
honored. Configuration entries for databases, CORS, caching, and rate limiting
are not implementations of those services. The todo example requires none of them.

## Verification

With stable Rust and the platform's C/C++ build tools installed:

```sh
cargo build --locked
cargo test --locked
```

The test harness binds `127.0.0.1:0`, sends real HTTP/1 requests, collects response
bodies, and asserts status, content type, and content. It uses independent temporary
projects, checks simultaneous clients with a barrier, and joins both server and
runtime threads on drop. Five-second request deadlines fail stuck tests; elapsed
time or process exit is never treated as successful verification.
