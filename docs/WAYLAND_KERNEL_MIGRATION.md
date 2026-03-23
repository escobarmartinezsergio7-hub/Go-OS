# Migracion A Wayland Nativo

## Estado actual del repo

- El compositor principal es propio y pinta por software sobre GOP/backbuffer en [kernel/src/framebuffer.rs](/Users/mac/Documents/New project/kernel/src/framebuffer.rs), [kernel/src/main.rs](/Users/mac/Documents/New project/kernel/src/main.rs) y [kernel/src/gui/compositor.rs](/Users/mac/Documents/New project/kernel/src/gui/compositor.rs).
- El runtime Linux ya carga ELF y expone un bridge grafico interno en [kernel/src/syscall.rs](/Users/mac/Documents/New project/kernel/src/syscall.rs).
- El entorno Linux esta forzado a X11 desde [kernel/src/linux_compat.rs](/Users/mac/Documents/New project/kernel/src/linux_compat.rs): `DISPLAY=:0`, `XDG_SESSION_TYPE=x11`, `GDK_BACKEND=x11`, `QT_QPA_PLATFORM=xcb`, `WINIT_UNIX_BACKEND=x11`, `MOZ_ENABLE_WAYLAND=0`, `WAYLAND_DISPLAY=`.
- Existe un subset X11 dentro del kernel: sockets virtuales `/tmp/.x11-unix/X0`, handshake X11, ventanas X11, pixmaps, propiedades y bridge de frames en [kernel/src/syscall.rs](/Users/mac/Documents/New project/kernel/src/syscall.rs).
- Hay demos Linux/X11 reales en [apps/linux_x11_demo/x11_demo.asm](/Users/mac/Documents/New project/apps/linux_x11_demo/x11_demo.asm) y [apps/linux_x11_anim_demo/x11_anim.asm](/Users/mac/Documents/New project/apps/linux_x11_anim_demo/x11_anim.asm).
- No existe implementacion Wayland en el arbol del kernel.
- No existe `virtio-gpu`; en [kernel/src/virtio/mod.rs](/Users/mac/Documents/New project/kernel/src/virtio/mod.rs) solo hay bloque, red e input stub.

## Aclaracion tecnica

Wayland no elimina el rol servidor. Lo mueve al compositor.

Si el objetivo es `kernel Rust -> compositor propio -> apps Linux`, entonces el compositor de ReduxOS debe implementar el lado servidor del protocolo Wayland. No hace falta Xorg, pero si hace falta ese extremo protocolo.

## Lo que falta de verdad

### 1. Separar salida de video de protocolo

Antes de Wayland conviene desacoplar:

- backend de scanout: GOP hoy, `virtio-gpu` o Intel despues
- compositor interno: ventanas, foco, damage, z-order
- protocolo guest: X11 hoy, Wayland despues

Mientras todo termine directo en `framebuffer.rs`, el backend grafico y el protocolo quedan mezclados.

### 2. Soporte AF_UNIX generico para Wayland

Hace falta un endpoint tipo `/run/wayland-0` o equivalente interno estable, con:

- `socket`
- `bind`
- `listen`
- `accept`
- `connect`
- colas de mensajes por cliente
- framing binario Wayland por objeto/opcode

El soporte actual esta muy orientado a X11 virtual.

### 3. Paso de FDs por Unix socket

Este es el bloqueo principal.

Wayland `wl_shm` necesita enviar FDs por `SCM_RIGHTS` usando `sendmsg`/`recvmsg`.

En el repo:

- `memfd_create` ya existe en [kernel/src/syscall.rs](/Users/mac/Documents/New project/kernel/src/syscall.rs).
- `sendmsg` y `recvmsg` tambien existen, pero no procesan `msg_control` ni `SCM_RIGHTS`.

Sin eso no puedes soportar `wl_shm` real.

### 4. Objetos basicos Wayland

Minimo necesario:

- `wl_display`
- `wl_registry`
- `wl_compositor`
- `wl_surface`
- `wl_buffer`
- `wl_shm`
- `wl_shm_pool`
- `wl_callback`
- `wl_seat`
- `wl_pointer`
- `wl_keyboard`
- `xdg_wm_base`
- `xdg_surface`
- `xdg_toplevel`

Sin `xdg-shell`, la mayoria de apps Wayland modernas no levantan ventana util.

### 5. Modelo de buffers y commit

El compositor actual ya sabe dibujar pixeles, pero Wayland necesita otra semantica:

- `create_buffer`
- `attach`
- `damage`
- `commit`
- `frame done`
- lifecycle por cliente

La unidad correcta ya no es "peticion de dibujo X11", sino `surface + buffer`.

### 6. Integracion con las ventanas nativas

No conviene crear otro window manager.

Lo correcto es mapear `xdg_toplevel` al sistema actual de ventanas en [kernel/src/gui/window.rs](/Users/mac/Documents/New project/kernel/src/gui/window.rs) y [kernel/src/gui/compositor.rs](/Users/mac/Documents/New project/kernel/src/gui/compositor.rs):

- titulo
- resize/configure
- focus
- move
- close
- maximize

### 7. Input Wayland real

El input actual debe exponerse como:

- `wl_seat`
- `wl_pointer`
- `wl_keyboard`

No basta con el bridge grafico interno actual.

### 8. Backend de salida futuro

Para la primera fase Wayland no necesitas GPU acelerada.

Ruta pragmatica:

1. conservar GOP/backbuffer
2. montar Wayland `wl_shm` sobre compositor software
3. despues agregar `virtio-gpu`
4. despues evaluar aceleracion Intel/direct scanout

## Orden recomendado

### Fase 0

Mantener X11 como compatibilidad temporal. No quitarlo todavia.

### Fase 1

Extraer una interfaz de surfaces Linux dentro del compositor.

### Fase 2

Completar `AF_UNIX` y agregar `SCM_RIGHTS` a `sendmsg/recvmsg`.

### Fase 3

Implementar Wayland minimo:

- `wl_display`
- `wl_registry`
- `wl_compositor`
- `wl_surface`
- `wl_shm`
- `wl_shm_pool`
- `wl_buffer`

Objetivo: cliente software que pinte un buffer.

### Fase 4

Agregar shell de ventanas:

- `xdg_wm_base`
- `xdg_surface`
- `xdg_toplevel`

### Fase 5

Agregar `wl_seat`, `wl_pointer` y `wl_keyboard`.

### Fase 6

Crear una demo nueva `apps/linux_wayland_demo/` y validar:

- ventana visible
- `wl_shm`
- input
- `frame callback`

## Lo que no conviene hacer ahora

- No activar `WAYLAND_DISPLAY` antes de tener servidor Wayland real.
- No quitar las variables X11 actuales hasta que exista una demo Wayland funcional.
- No empezar por XWayland.
- No empezar por `virtio-gpu`; primero hay que cerrar protocolo y buffers.

## Primeros archivos a tocar

- [kernel/src/syscall.rs](/Users/mac/Documents/New project/kernel/src/syscall.rs): promover `AF_UNIX` a backend generico y agregar `SCM_RIGHTS` a `sendmsg/recvmsg`.
- [kernel/src/linux_compat.rs](/Users/mac/Documents/New project/kernel/src/linux_compat.rs): dejar preparada la seleccion futura de backend sin quitar todavia el forzado actual a X11.
- [kernel/src/gui/compositor.rs](/Users/mac/Documents/New project/kernel/src/gui/compositor.rs): introducir una capa de surfaces Linux independiente del bridge X11.
- [kernel/src/gui/window.rs](/Users/mac/Documents/New project/kernel/src/gui/window.rs): mapear una surface externa a la ventana nativa y su ciclo de `configure/commit/focus`.
- [kernel/src/framebuffer.rs](/Users/mac/Documents/New project/kernel/src/framebuffer.rs): mantenerlo como backend software inicial, pero ya detras de una abstraccion de salida.
- [apps/](/Users/mac/Documents/New project/apps): agregar una demo Wayland minima cuando exista `wl_shm`.

## Criterio de exito minimo

La migracion es real cuando un ELF Linux:

- arranca con `WAYLAND_DISPLAY=wayland-0`
- conecta por `AF_UNIX` al compositor interno
- crea `wl_surface` + `xdg_toplevel`
- envia un buffer `wl_shm`
- se ve como ventana normal en ReduxOS
- recibe teclado y raton
- ya no depende de `DISPLAY=:0`
