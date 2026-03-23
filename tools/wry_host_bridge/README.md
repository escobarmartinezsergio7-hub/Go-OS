# wry_host_bridge

Runtime host WebKit/Wry para render web real (event loop + ventana nativa) fuera del kernel UEFI.

## Uso

```bash
cargo run --manifest-path tools/wry_host_bridge/Cargo.toml -- --bind 127.0.0.1:37810 --url https://www.google.com
```

Script auxiliar:

```bash
bash scripts/run_wry_host_bridge.sh 0.0.0.0:37810 https://www.google.com
bash scripts/run_webkit_host_bridge.sh 0.0.0.0:37810 https://www.google.com
```

## Endpoints de control

- `GET /status`
- `GET /open?url=https://...`
- `GET /eval?js=...`
- `GET /input?type=back|forward|reload|scroll|click|key|text`
- `GET /frame` (macOS: captura frame y responde PPM P6)
- `GET /quit`

Ejemplo:

```bash
curl "http://127.0.0.1:37810/status"
curl "http://127.0.0.1:37810/open?url=https%3A%2F%2Ftauri.app"
curl "http://127.0.0.1:37810/input?type=reload"
curl "http://127.0.0.1:37810/frame" --output frame.ppm
```

Nota:
- La captura `/frame` esta implementada para `WKWebView` en macOS.
- En otros sistemas operativos puede devolver error de no soportado.

## Nota

Este binario corre en host (macOS/Linux/Windows) y requiere backend gráfico del sistema.
No corre dentro del kernel UEFI `no_std`.
