# HTTP request-target parser patches

`hyper` 1.11.0 and `h2` 0.4.19 are patched locally because their request parsers
pass raw request targets through `http::Uri` / `PathAndQuery`, which removes a
fragment before application middleware can inspect it. Both protocols now reject
raw `#` in the request target before that normalization. Encoded `%23` remains
valid. This prevents malformed destructive requests such as
`DELETE /dav/item#suffix` from being applied to `/dav/item`.

These are focused patches to the upstream parser entry points; the generic `http`
URI behavior is unchanged. When updating either dependency, retain the parser
regressions and the litmus fragment-delete assertion in CI.
