# Go OS

*Actualmente en desarrollo.*

**Go OS** es un sistema operativo experimental, moderno y ligero desarrollado desde cero. Está diseñado para ser rápido, eficiente y visualmente atractivo. Cuenta con su propio kernel y una interfaz de usuario gráfica (GUI) completa y funcional.

## Características Principales

* **Kernel Custom:** Un núcleo escrito íntegramente en Rust que gestiona la memoria, interactúa con el hardware, controla los procesos y da soporte al sistema de archivos de manera segura.
* **Interfaz Gráfica de Usuario (GUI):** Un compositor de ventanas personalizado que incluye un entorno de escritorio completo:
  * Barra de tareas con iconos personalizables y menú de inicio rápido.
  * Soporte para ventanas múltiples, superposición e interacción mediante ratón y teclado.
  * Menús contextuales nativos tanto en el escritorio como en el explorador de archivos.
  * Renderizado de texto y fuentes, con soporte avanzado usando *LiteHTML* para ciertas integraciones web.
* **Explorador de Archivos:** Permite la navegación en unidades físicas (como USB en formato FAT32) y discos virtuales. Soporta arrastrar, soltar, copiar, y pegar archivos/directorios entre diferentes volúmenes físicos de forma robusta.
* **Redux Studio (IDE):** Un entorno de desarrollo integrado nativo que permite escribir, manejar código y guardar archivos, con sistema de pestañas y acceso al portapapeles global del OS.
* **Herramientas de Sistema:**
  * **Notepad:** Un bloc de notas ligero para visualización y edición rápida de archivos de texto plano.
  * **Consola / Terminal:** Capaz de interactuar con el sistema de archivos de manera asíncrona, soporta un esquema de trabajo similar a los shells UNIX, así como comandos UEFI.
  * **Multimedia:** Visores de imágenes integrados (formatos como PNG) y reproductores/decodificadores de audio iterativos.

## Cómo Funciona

El sistema arranca utilizando el estándar UEFI con un bootloader personalizado. Después de elegir la partición de arranque, se presenta un **Boot Splash** visual mientras el kernel inicializa estructuras vitales como el *allocator* de memoria y detecta los dispositivos de almacenamiento. A continuación, el **Compositor Gráfico** toma el control del *framebuffer* de UEFI para dibujar la GUI y gestionar todos los eventos de hardware (teclado, ratón). Utiliza un sistema cooperativo en el que aplicaciones núcleo como explorar, jugar o utilizar el Editor de Texto / Studio actúan integrados con este mismo bucle principal bajo la arquitectura de `Windows` y widgets nativos propios de Go OS.

## Lenguajes de Programación Utilizados

* **Rust:** Es el corazón de **Go OS**. Prácticamente toda la base del proyecto, desde las subrutinas de bajo nivel del kernel, el manejo de interrupciones, los controladores del sistema de archivos (FAT32), hasta la construcción del Compositor visual y todas las utilidades de escritorio (Explorador, Taskbar) están desarrollados en código Rust seguro y eficiente.
* **Ensamblador (x86_64 Assembly):** Utilizado para algunas rutinas de inicialización de muy bajo nivel, arranque y contextos de interrupción.
* **HTML / CSS / JS / Ruby:** Involucrados dentro de los soportes experimentales para la interpretación web con el motor integrado (*LiteHTMLBridge*) y el emulador en el entorno *Linux Runtime* (capa de compatibilidad opcional para correr apps portadas).

---
*Desarrollado y mantenido por Emmanuel Escobar Ochoa y contribuyentes de código abierto.*

---

## Guía Técnica y Build

Starter de SO experimental con dos rutas:

- **UEFI x86_64 (principal):** Rust `no_std` sobre `x86_64-unknown-uefi` + QEMU/OVMF

### Estado actual

Este repo ahora esta ajustado para **x86_64 + UEFI (OVMF)** como camino principal de prueba.

### Estructura clave

- `kernel/src/main.rs`: app UEFI en Rust (entry EFI)
- `kernel/src/memory.rs`: parser de memory map UEFI + frame allocator basico
- `kernel/src/interrupts.rs`: IDT + PIC + IRQ0 real
- `kernel/src/timer.rs`: tick clock + PIT (hardware)
- `kernel/src/scheduler.rs`: scheduler cooperativo de diagnostico
- `kernel/src/process.rs`: modelo de procesos/hilos (userspace)
- `kernel/src/syscall.rs`: tabla de syscalls + dispatcher + estadisticas
- `kernel/src/usermode.rs`: shell/app de usuario (Ring 3 logico)
- `kernel/src/privilege.rs`: GDT + TSS + SYSCALL/SYSRET + gate INT 0x80
- `kernel/src/framebuffer.rs`: primitives GOP framebuffer
- `kernel/src/ui.rs`: compositor simple de escritorio + taskbar
- `kernel/src/runtime.rs`: runtime post-EBS (sin boot services)
- `kernel/src/hal.rs`: puertos I/O + instrucciones CPU (`hlt`, `sti`, `cli`)
- `scripts/run_uefi.sh`: runner QEMU + deteccion OVMF
- `Makefile`: targets UEFI por defecto
- `Makefile.legacy-bios`: snapshot historico BIOS/multiboot (referencia)
- `kernel/src/legacy_bios.rs`: shell legacy BIOS (referencia historica)
- `tools/`: empaquetador/instalador `.rpx` (Ruby)
- `apps/hello_redux/`: app de ejemplo (`.rml` + `.rdx`)
- `sdk/reduxlang/`: lexer+parser+evaluator ejecutable

### Dependencias UEFI (principal)

- `rustup`
- target Rust: `x86_64-unknown-uefi`
- `qemu-system-x86_64`
- firmware OVMF/edk2
- `ruby` (para herramientas de paquetes)

### Build UEFI

```bash
rustup target add x86_64-unknown-uefi
make clean
make uefi
```

Esto genera:

- `build/esp/EFI/BOOT/BOOTX64.EFI`
- `build/esp/LINUXRT` (si existe `LINUXRT/` en la raiz del repo)
- `build/esp/EFI/LINUX/BOOTX64.EFI` (si ejecutas `make linux-guest-stage` o activas `LINUX_GUEST_AUTO=1`)

#### Linux guest EFI (ruta 2)

Staging de un kernel Linux EFI/bzImage precompilado al ESP:

```bash
make linux-guest-stage LINUX_GUEST_TREE=/Users/mac/Downloads/linux-6.19.3
# o con archivo explicito:
make linux-guest-stage LINUX_GUEST_EFI_INPUT=/ruta/a/bzImage
```

Compilar Linux y stage en una sola pasada (host Linux recomendado):

```bash
make linux-guest-build LINUX_GUEST_TREE=/ruta/linux-6.19.3
```

### Ejecutar en QEMU + OVMF

```bash
make run
```

El script `scripts/run_uefi.sh` detecta OVMF en rutas comunes de Linux/macOS.

### Instalador grafico pre-boot (antes del kernel)

Al arrancar, ReduxOS ahora muestra un instalador grafico UEFI antes del shell.

Funciones:

- seleccionar particion objetivo
- redimensionar particion seleccionada (pasos de 128 MiB, MBR)
- crear nueva particion en el mayor espacio libre (MBR)
- instalar paquete base de sistema en la particion elegida:
  - `\EFI\BOOT\BOOTX64.EFI`
  - `\startup.nsh`
  - `\REDUXOS.INI` (marcador/autoboot)
  - `\README.TXT`
  - `\LINUXRT\...` (obligatorio; se toma del bundle embebido generado en build)

Controles:

- `N/P`: mover seleccion
- `+` o `=`: crecer particion seleccionada
- `-`: reducir particion seleccionada
- `C`: crear particion nueva en espacio libre
- `R`: recargar tablas de disco/particiones
- `1..9`: seleccionar objetivo directo
- `Enter` (doble confirmacion): instalar
- `Esc`: omitir instalador y continuar

Limitaciones actuales:

- soporta deteccion e instalacion en **GPT y MBR**
- operaciones de **redimensionar/crear** siguen en modo **MBR** por ahora

### Instalar en particion NVMe interna (hardware real, Linux)

Este flujo instala ReduxOS en una **particion NVMe interna ya existente**.

**Advertencia:** el instalador formatea la particion objetivo en FAT32 y borra todos sus datos.

#### Requisitos

- Linux (el script usa `lsblk`, `findmnt`, `mkfs.fat`, `mount`, `umount`)
- permisos de `sudo`
- particion NVMe destino creada previamente (ej: `/dev/nvme0n1p4`)

#### Uso rapido

```bash
make uefi
sudo bash scripts/install_nvme.sh --partition /dev/nvme0n1p4
```

Alternativa con Makefile:

```bash
make install-nvme PARTITION=/dev/nvme0n1p4
```

Opcional: cambiar etiqueta FAT32:

```bash
make install-nvme PARTITION=/dev/nvme0n1p4 NVME_INSTALL_LABEL=REDUXEFI
```

El instalador:

- valida que el destino sea una particion NVMe interna
- evita particiones activas de sistema (`/`, `/boot`, `/boot/efi`)
- desmonta la particion si esta montada
- formatea en FAT32
- copia `BOOTX64.EFI` a `EFI/BOOT/BOOTX64.EFI`
- crea `startup.nsh` con la ruta UEFI estandar
- crea `REDUXOS.INI` (marcador de instalacion/autoboot)
- crea `README.TXT` con informacion basica del sistema
- copia `LINUXRT` desde `build/esp/LINUXRT` hacia la particion (**obligatorio**)

Opciones utiles:

- `--linuxrt-source /ruta/a/LINUXRT` para usar otra carpeta runtime
- `--linux-guest-source /ruta/a/EFI/LINUX` para copiar tambien loader Linux guest

### Comandos de shell (Phase 1)

Al arrancar en QEMU veras prompt `redux>`.

- `help`
- `about`
- `clear`
- `mem`
- `alloc`
- `idt`
- `tick`
- `sched`
- `step`
- `boot` (salta a runtime kernel: ExitBootServices + GUI en modo polling estable)
- `boot uefi` (GUI sin ExitBootServices: input UEFI, util para teclados USB en hardware real)
- `boot irq` (activa PIT/IRQ real; conserva fallback automatico de seguridad a polling)
- `echo <text>`
- `panic`
- `reboot`
- `disks` (lista dispositivos BlockIO USB/NVMe/HDD, FAT32 o no)
- `vols` (lista volumenes FAT32 montables)
- `mount <n>` (monta volumen FAT32 por indice para `ls/cd/cat`)

### Runtime kernel (Phase 2/3)

Al ejecutar `boot`, el flujo hace:

1. Captura framebuffer GOP
2. `ExitBootServices`
3. Re-inicializa memoria desde el memory map final
4. Entra a loop propio con scheduler + render GUI base (polling o IRQ segun comando)

En `boot uefi`, el flujo hace:

1. Captura framebuffer GOP
2. **No** llama `ExitBootServices` (mantiene Boot Services)
3. Entra al mismo GUI pero usando **input UEFI** (teclado USB funciona)

Nota: `boot uefi` es un "dev-mode" para hardware real hasta que exista stack USB (xHCI/HID).

En `boot irq`, el contador de IRQ de la UI debe incrementarse de forma continua.

La GUI actual es un compositor minimo:

- fondo de escritorio
- dos ventanas demo
- barra de tareas
- indicador animado
- metricas numericas (ticks, dispatches, IRQ, memoria)
- texto bitmap 5x7 renderizado por CPU (sin GPU acelerada)
- doble buffer software (backbuffer + present)
- terminal integrada con shell en userspace via syscalls

Capas de privilegio activas en runtime:

- GDT propia (kernel/user)
- TSS cargado con stack de Ring 0
- ruta `SYSCALL/SYSRET` configurada via MSR
- gate de usuario `INT 0x80` (DPL=3) instalado en IDT

Comandos dentro de la terminal runtime:

- `help`
- `clear`
- `about`
- `status`
- `echo <texto>`
- `ps`
- `syscalls`
- `priv` (estado de fases de privilegio hardware)
- `priv next` (avanza una fase: GDT/TSS -> gates -> MSR syscall -> test CPL3)
- `priv unsafe` (ejecuta test CPL3 real; puede ser inestable)
- `fetch <url> [file_8_3]` (terminal GUI: descarga archivos HTTP/HTTPS completos; limite actual 4 MiB por archivo)
- `web backend <builtin|vaev|webkit|status>` (terminal GUI: selecciona motor del Web Explorer)
- `web vaev status` (estado del bridge Vaev embebido)
- `web native <on|off|status>` (activa/desactiva pipeline nativo DOM/layout/raster interno)
- `install <package.rpx|package.zip|package.tar|package.tar.gz|package.deb|setup.exe> [app_id]` (terminal GUI: instala paquetes descargados y genera manifiestos `.LST`/`.LNX`)
- `ruby -e <code>` / `ruby <file.rb>` (terminal GUI: subset Ruby embebido para scripts simples)

Backend Servo para Web Explorer:

- modo por defecto: `builtin` (render interno)
- modo opcional: `servo` (adaptador estilo API Rust `servo::Servo` + `webview::WebView`, con fallback automatico a `builtin`)
- `servo` ahora puede entregar una superficie grafica al viewport del navegador (`FRAME_MODE/FRAME_SIZE`) ademas del texto
- activar en build: `cargo build --manifest-path kernel/Cargo.toml --features servo_bridge`
- para forzar enlace con libreria externa real:
  - `cargo build --manifest-path kernel/Cargo.toml --features "servo_bridge,servo_external"`
  - opcional: `SERVO_LIB_DIR=/ruta/a/lib` (si no, usa `kernel/third_party/servo/lib`)
- este repo ya incluye un shim integrado (`simpleservo_shim`) que mapea el flujo Rust API (build/webview/spin/paint) sobre el renderer interno para validar el pipeline end-to-end
- si `servo_external` esta activo pero no encuentra `libsimpleservo`, el build hace fallback automatico al shim
- el bridge espera estos simbolos C en el link final:
  - `simpleservo_bridge_is_ready() -> i32`
  - `simpleservo_bridge_render_text(url_ptr, url_len, out_ptr, out_cap, out_len) -> i32`

Backend Vaev embebido (kernel):

- comando en ReduxOS: `web backend vaev`
- diagnostico: `web vaev status`
- build embebido (shim integrado):

```bash
cargo build --manifest-path kernel/Cargo.toml --target x86_64-unknown-uefi --features vaev_bridge
```

- build embebido enlazando libreria externa (opcional):

```bash
cargo build --manifest-path kernel/Cargo.toml --target x86_64-unknown-uefi --features "vaev_bridge,vaev_external"
```

- opcional: `VAEV_LIB_DIR=/ruta/a/lib` (si no, usa `kernel/third_party/vaev/lib`)
- el bridge espera estos simbolos C en el link final:
  - `vaev_bridge_is_ready() -> i32`
  - `vaev_bridge_render_text(url_ptr, url_len, out_ptr, out_cap, out_len) -> i32`
- si `vaev_external` esta activo pero no encuentra `libvaevbridge`, el build hace fallback automatico al shim integrado.

Cutekit para compilar Vaev fuera del kernel (host):

```bash
python3 -m venv .venv-vaev
source .venv-vaev/bin/activate
python -m pip install git+https://codeberg.org/cute-engineering/cutekit
```

- en macOS (Homebrew LLVM), agrega:
  - `export PATH="/opt/homebrew/opt/llvm/bin:$PATH"`

Backend WebKit/Wry (host runtime, recomendado en macOS):

- comando en ReduxOS: `web backend webkit`
- aliases soportados: `wry ...` y `web cef ...` (legacy)
- en build UEFI/no_std, `GO` envia la URL al host bridge via HTTP (`/open?url=...`) para render real HTML/CSS/JS en ventana host.
- para render real con Wry, usar host runtime en macOS/Linux/Windows con event loop de ventana (`tao`) y WebView nativo.
- comandos de control en ReduxOS:
  - `web webkit status`
  - `web webkit endpoint <http://host:port|auto>`
  - `web webkit ping`
  - `web webkit open <url>`
  - `web webkit input <click x y|scroll d|key K|text T|back|forward|reload>`
- endpoint por defecto en ReduxOS: `http://10.0.2.2:37810` (util para QEMU guest -> host).

Motor nativo interno (sin dependencia externa):

- el renderer interno ahora expone una superficie grafica `native-dom-layout-raster-v1` generada dentro de ReduxOS.
- flujo: parse HTML -> construir tokens DOM -> layout por bloques -> raster software a framebuffer de ventana.
- comando de control en ReduxOS:
  - `web native status`
  - `web native on`
  - `web native off`

Host bridges opcionales incluidos en este repo:

- crate: `tools/wry_host_bridge`
- script: `scripts/run_wry_host_bridge.sh`
- script alias: `scripts/run_webkit_host_bridge.sh`
- bridge Ruby para Vaev: `tools/vaev_host_bridge`
- script: `scripts/run_vaev_host_bridge.sh`
- API local HTTP:
  - `GET /status`
  - `GET /open?url=https://...`
  - `GET /eval?js=...`
  - `GET /quit`

Arranque rapido del host bridge:

```bash
bash scripts/run_webkit_host_bridge.sh 0.0.0.0:37810 https://www.google.com
```

Prueba de control:

```bash
curl "http://127.0.0.1:37810/status"
curl "http://127.0.0.1:37810/open?url=https%3A%2F%2Ftauri.app"
```

Backend CEF C++ (host runtime):

- ruta recomendada para navegador real en C++: `tools/cef_host_bridge`
- build con CMake contra distribucion binaria de CEF (`CEF_ROOT`)
- script de arranque:

```bash
export CEF_ROOT=/ruta/a/cef_binary
bash scripts/run_cef_host_bridge.sh 127.0.0.1:37820 https://www.google.com
```

- empaquetado de runtime para USB:

```bash
bash scripts/package_cef_runtime_usb.sh /Volumes/REDUXOS /ruta/a/cef_binary
```

- docs detalladas: `tools/cef_host_bridge/README.md`
- API local bridge WebKit/Wry (fase actual): `/status`, `/open`, `/eval`, `/input`, `/frame`, `/quit`
  - `GET /frame` devuelve imagen PPM (P6) en macOS con WKWebView.
- docs del bridge Vaev: `tools/vaev_host_bridge/README.md`

Atajos de teclado en runtime grafico:

- `F1`: pausa/reanuda animacion y scheduler visual
- `Enter`: envia comando en terminal
- `Backspace`: borra en terminal
- `Space`: avanza 1 tick cuando esta en pausa (modo polling)
- `F2`: alterna tema visual
- `Esc`: reinicia la VM (reset por controlador de teclado)

### Paquetes `.rpx` / `.zip` (Ruby)

Generar paquete de ejemplo:

```bash
ruby tools/bootstrap_repo.rb
```

Flujo minimo estilo recipe (`recipe.toml` + builder + firma):

```bash
# receta de ejemplo
cat recipes/hello_redux/recipe.toml

# construir .rpx + .sig
ruby tools/redux_recipe_build.rb recipes/hello_redux/recipe.toml
```

Notas de firma:

- El builder genera `REDUX-SIG-V1` en `<paquete>.sig` con `SHA256`.
- En runtime, `install <paquete>` valida automaticamente `<paquete>.sig` si existe.
- Si la firma existe y no coincide, la instalacion se cancela.

Actualizar catalogo:

```bash
ruby tools/redux_get.rb update
```

Instalar app local:

```bash
ruby tools/redux_get.rb install hello-redux
```

`redux_get install` tambien valida `packages/<app>.rpx.sig` cuando esta presente.

Listar instaladas:

```bash
ruby tools/redux_get.rb list
```

Smoke test Linux ELF (sin internet, usando binario local):

```bash
# genera paquete firmado de prueba
ruby tools/redux_recipe_build.rb recipes/linux_sandbox_smoke/recipe.toml
```

En ReduxOS (terminal):

```text
install LINUX_SANDBOX_SMOKE.RPX LSBX
cat LSBX.LST
linux inspect /LSBX/LSBX0001.BIN
linux runloop start /LSBX/LSBX0001.BIN
linux runloop status
```

Notas para `.zip` en runtime:

- Soporta entradas sin compresion (metodo ZIP `0` / stored)
- Soporta entradas comprimidas con deflate (metodo ZIP `8`)
- No soporta data descriptor (bit general purpose `3`)
- Limite actual de paquete: `8 MiB`

### SDK ReduxLang

```bash
cargo run --manifest-path sdk/reduxlang/Cargo.toml
```

Ejemplo en REPL:

```text
let x = 7 * (3 + 2);
x + 10;
```

### Porting C++ con newlib (fase1 estatico)

Se agrego un kit para portar apps C++ estaticas al perfil Linux ELF fase1:

- `sdk/newlib_cpp/crt0.S`
- `sdk/newlib_cpp/newlib_syscalls.cpp`
- `sdk/newlib_cpp/build_app.sh`
- `scripts/newlib_port.sh`

Flujo recomendado:

```bash
# 1) generar plantilla
bash scripts/newlib_port.sh scaffold miapp

# 2) compilar app C++ con toolchain newlib (x86_64-elf)
bash scripts/newlib_port.sh build apps/newlib/miapp/main.cpp build/newlib_cpp/MIAPP.BIN

# 3) validar perfil ELF antes de copiar al disco/USB
bash scripts/newlib_port.sh doctor build/newlib_cpp/MIAPP.BIN
```

Objetivo tecnico del binario:

- `ELF64 x86_64`
- `ET_EXEC` (sin PIE)
- sin `PT_INTERP`
- sin `PT_DYNAMIC`

En ReduxOS, verifica con:

```text
linux inspect /MIAPP.BIN
```
