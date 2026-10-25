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
reports a diagnostic and retains the complete last good snapshot. HTML/route-only
edits preserve module globals. Changing a module's `global` block resets that
module's state; removal discards it. Development checks read the source tree on
each request and are intended for correctness and convenience, not throughput.
