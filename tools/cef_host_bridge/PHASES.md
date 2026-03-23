# Ruta exacta (CEF -> ReduxOS)

## Fase 1 (hecha en este commit)

- App C++ base con CEF: abre URL real y corre loop de navegador.
- Build portable con CMake (`CEF_ROOT`).
- Scripts:
  - `scripts/run_cef_host_bridge.sh`
  - `scripts/package_cef_runtime_usb.sh`

## Fase 2 (hecha parcialmente en este commit)

- Agregar API de control al host bridge:
  - `GET /status`
  - `GET /open?url=...`
  - `POST /input` (mouse/teclado)
  - `GET /frame` (frame RGBA actual)
- Estado actual:
  - `/status`, `/open`, `/eval`, `/input`, `/quit`: activos.
  - `/frame`: activo (PPM P6 desde CEF OSR `OnPaint`).
- Meta inmediata: ReduxOS puede controlar navegacion remota desde boton GO.

## Fase 3 (siguiente bloque)

- Integrar backend `cef-host` en Web Explorer del kernel:
  - comando `web backend cef`
  - endpoint configurable (`web cef endpoint <url>`)
  - fallback seguro a `builtin` si host no responde.

## Fase 4

- Interactividad completa:
  - clic/scroll/teclado desde ventana ReduxOS al host CEF
  - streaming de frame incremental (dirty rects) para rendimiento.

## Fase 5

- Empaquetado final:
  - bundle de runtime CEF + binario host
  - copia a USB/disco con script automatizado
  - checklist de validacion (`status/open/input/frame`).
