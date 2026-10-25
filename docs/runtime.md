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
