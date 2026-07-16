# Linux Wayland GTK Demo

Demo minimo GTK3 para validar cliente Wayland sobre el compositor interno.

## Build (host Linux recomendado)

```bash
cd apps/linux_wayland_gtk_demo
./build.sh
```

Salida esperada: `GTKWLDMO.BIN` (ELF dinamico PIE con `PT_INTERP`).

## Dependencias de build

```bash
sudo apt update
sudo apt install -y build-essential pkg-config libgtk-3-dev
```

## Ejecutar en ReduxOS

1. Copia `GTKWLDMO.BIN` al volumen de ReduxOS (raiz o carpeta conocida).
2. En terminal ReduxOS:

```text
linux inspect /GTKWLDMO.BIN
linux runloop startx /GTKWLDMO.BIN
```

Si la ventana aparece, el path Wayland (`wayland-0`) esta operativo para GTK.
