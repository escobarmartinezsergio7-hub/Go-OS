Place external Vaev bridge libraries here when using:

`cargo build --manifest-path kernel/Cargo.toml --features "vaev_bridge,vaev_external"`

Expected default path:

- `kernel/third_party/vaev/lib/libvaevbridge.a` (or `vaevbridge.lib`)

You can override the search path with:

- `VAEV_LIB_DIR=/custom/path`

Required exported C symbols:

- `vaev_bridge_is_ready() -> i32`
- `vaev_bridge_render_text(const u8*, usize, *mut u8, usize, *mut usize) -> i32`

Notes:

- If `vaev_external` is enabled and the library is missing, build falls back to the integrated Rust shim (`vaevbridge_shim`).
- The integrated shim still enables `web backend vaev` without external binaries.
