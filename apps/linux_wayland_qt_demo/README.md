# Linux Wayland Qt Demo

Demo minimo Qt Widgets para validar cliente Wayland sobre el compositor interno.

## Build (host Linux recomendado)

```bash
cd apps/linux_wayland_qt_demo
./build.sh
```

Salida esperada: `QTWLDMO.BIN` (ELF dinamico PIE con `PT_INTERP`).

## Dependencias de build

```bash
sudo apt update
sudo apt install -y build-essential pkg-config qtbase5-dev qtwayland5
```

## Ejecutar en ReduxOS

1. Copia `QTWLDMO.BIN` al volumen de ReduxOS (raiz o carpeta conocida).
2. En terminal ReduxOS:

```text
linux inspect /QTWLDMO.BIN
linux runloop startx /QTWLDMO.BIN
```

Si la ventana aparece, el path Wayland (`wayland-0`) esta operativo para Qt.
