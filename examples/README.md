# Examples

Runnable usage examples for `mpd-schema` and `mpd-resolve`. None depend on the
fetched fixtures — each carries its own inline manifest.

| Example | What it shows | Run |
| --- | --- | --- |
| `schema_build` | Build an MPD from scratch (`new` + public fields) and serialize it | `cargo run -p examples --example schema_build` |
| `schema_roundtrip` | Parse, edit a known attribute, and write back — preserving an unknown `cenc:pssh` DRM node verbatim | `cargo run -p examples --example schema_roundtrip` |
| `resolve_segments` | Resolve a manifest into concrete segment URLs (init + media) | `cargo run -p examples --example resolve_segments` |

`resolve_segments` also accepts your own manifest. The resolver needs an
absolute base URL to resolve relative `BaseURL`s, so pass the file and its URL:

```sh
cargo run -p examples --example resolve_segments -- manifest.mpd https://cdn.example.com/manifest.mpd
```

With no arguments it resolves a small embedded manifest.
