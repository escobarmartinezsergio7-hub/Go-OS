# cef_host_bridge (C++)

Bridge en C++ con CEF (OSR/windowless) para render web real (HTML/CSS/JS) via HTTP.

## Objetivo

- Tener una ruta practica para navegador real sin reimplementar un motor web completo en UEFI/no_std.
- Compilar una app host C++ (`cef_host_bridge`) que abra URLs reales.
- Preparar empaquetado para USB y despliegue manual.

## Requisitos

- CMake >= 3.16
- Compilador C++17
- CEF binary distribution (incluye `include/` y runtime de `libcef`)

## Build

Define `CEF_ROOT` al directorio base extraido de CEF:

```bash
cmake -S tools/cef_host_bridge -B build/cef_host_bridge -DCEF_ROOT=/ruta/a/cef_binary
cmake --build build/cef_host_bridge -j
```

## Run

```bash
./build/cef_host_bridge/cef_host_bridge --bind 0.0.0.0:37810 --url https://www.google.com
```

Si no pasas URL, usa `https://www.google.com`.

## API HTTP local (fase 2)

- `GET /status`
- `GET /open?url=https://...`
- `GET /eval?js=...`
- `GET /input?type=text&text=hola`
- `GET /input?type=key&key=Enter`
- `GET /input?type=click&x=120&y=80`
- `GET /input?type=scroll&delta=200`
- `GET /frame` (devuelve frame actual como `image/x-portable-pixmap` P6)
- `GET /quit`

Ejemplos:

```bash
curl "http://127.0.0.1:37810/status"
curl "http://127.0.0.1:37810/open?url=https%3A%2F%2Fwww.google.com"
curl "http://127.0.0.1:37810/input?type=text&text=reduxos"
curl -o frame.ppm "http://127.0.0.1:37810/frame"
```

## Nota de integracion con ReduxOS

Este binario es host-side (macOS/Linux/Windows) y expone control remoto para ReduxOS.
