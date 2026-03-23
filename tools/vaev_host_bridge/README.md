# vaev_host_bridge (Ruby)

Bridge host-side para conectar ReduxOS con `vaev-browser` usando la misma API HTTP
que ya consume el backend remoto (`/status`, `/open`, `/quit`).

## Objetivo

- Reusar el flujo de `web backend` de ReduxOS sin incrustar el engine en UEFI/no_std.
- Lanzar Vaev en host (macOS/Linux) al recibir `GET /open?url=...`.

## Requisitos

- Ruby (para el servidor HTTP local).
- Proyecto Vaev disponible en disco (por defecto: `/Users/mac/Documents/vaev`).
- `python -m cutekit` instalado para ejecutar `vaev-browser`, o un binario directo
  configurado con `VAEV_BROWSER_BIN`.

## Uso

Script recomendado:

```bash
bash scripts/run_vaev_host_bridge.sh 0.0.0.0:37810 https://www.google.com /Users/mac/Documents/vaev
```

Ejecucion directa:

```bash
ruby tools/vaev_host_bridge/vaev_host_bridge.rb \
  --bind 0.0.0.0:37810 \
  --url https://www.google.com \
  --vaev-dir /Users/mac/Documents/vaev
```

## API HTTP local

- `GET /status`
- `GET /open?url=https://...`
- `GET /quit`

Compatibilidad:

- `/eval`, `/input`, `/frame` responden `501` (no implementado).

## Variables de entorno

- `VAEV_DIR`: path del proyecto Vaev.
- `VAEV_PYTHON`: ejecutable de Python para `-m cutekit`.
- `VAEV_BROWSER_BIN`: binario ejecutable de Vaev precompilado (si no usas cutekit).

## Integracion con ReduxOS

En el runtime de ReduxOS:

```text
web backend vaev
web cef endpoint http://10.0.2.2:37810
```

Luego usa el boton GO del Web Explorer.
