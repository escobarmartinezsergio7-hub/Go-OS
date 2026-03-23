Place external LiteHTML bridge libraries here when using:

`cargo build --manifest-path kernel/Cargo.toml --features "litehtml_bridge,litehtml_external"`

Expected default path:

- `kernel/third_party/litehtml/lib/liblitehtmlbridge.a` (or `litehtmlbridge.lib`)

You can override the search path with:

- `LITEHTML_LIB_DIR=/custom/path`

Required exported C symbols:

- `litehtml_bridge_is_ready() -> i32`
- `litehtml_bridge_render_text(const u8*, usize, *mut u8, usize, *mut usize) -> i32`

Optional upstream sync helper:

- `bash scripts/sync_litehtml_upstream.sh`
- `make litehtml-sync`

Build helper for external bridge archive:

- `bash scripts/build_litehtml_bridge.sh`
- `make litehtml-bridge-build`

This syncs upstream source into:

- `kernel/third_party/litehtml/upstream`

Notes:

- If `litehtml_external` is enabled and the library is missing, build falls back to the integrated Rust shim (`litehtmlbridge_shim`).
- The integrated shim still enables `web backend litehtml` without external binaries.
- Upstream `litehtml` is C++ and normally requires a dedicated bridge build step to produce `liblitehtmlbridge.a` for the kernel target.
