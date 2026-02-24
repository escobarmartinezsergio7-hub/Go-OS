Place external Servo bridge libraries here when using:

`cargo build --manifest-path kernel/Cargo.toml --features "servo_bridge,servo_external"`

Expected default path:

- `kernel/third_party/servo/lib/libsimpleservo.a` (or `simpleservo.lib`)

You can override the search path with:

- `SERVO_LIB_DIR=/custom/path`

Required exported C symbols:

- `simpleservo_bridge_is_ready() -> i32`
- `simpleservo_bridge_render_text(const u8*, usize, *mut u8, usize, *mut usize) -> i32`

Rust API mapping used by ReduxOS (`web backend servo`):

- `servo::Servo` lifecycle is represented as an adapter session (`build + spin_event_loop`)
- `webview::WebView` lifecycle is represented as URL load + paint step
- current low-level bridge transport remains text render payload over the symbols above
