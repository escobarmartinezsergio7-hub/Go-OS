# Newlib C++ Porting Kit (ReduxOS)

Este kit agrega una ruta practica para portar aplicaciones C++ estaticas
compiladas con toolchain `newlib` hacia ReduxOS (perfil Linux ELF fase1).

## Objetivo

Generar binarios:

- `ELF64 x86_64`
- `ET_EXEC` (no PIE)
- `static` (sin `PT_INTERP`, sin `PT_DYNAMIC`)

Ese perfil coincide con la validacion actual de `linux inspect`/fase1.

## Requisitos en host

- Toolchain `x86_64-elf` con `newlib` y `libstdc++` (`x86_64-elf-g++`, etc.)
- `readelf` (o `x86_64-elf-readelf`)
- Bash

## Uso rapido

Desde la raiz del repo:

```bash
bash scripts/newlib_port.sh build sdk/newlib_cpp/examples/hello_cpp.cpp
```

Salida por defecto:

```text
build/newlib_cpp/NEWLIBAPP.BIN
```

## Crear tu app C++

```bash
bash scripts/newlib_port.sh scaffold miapp
bash scripts/newlib_port.sh build apps/newlib/miapp/main.cpp build/newlib_cpp/MIAPP.BIN
```

## Validar compatibilidad del binario

```bash
bash scripts/newlib_port.sh doctor build/newlib_cpp/MIAPP.BIN
```

## Ejecutar validacion en ReduxOS

Con el binario ya copiado a tu volumen ReduxOS:

```text
linux inspect /MIAPP.BIN
```

Si reporta `Phase1 check: compatible para carga estatica`, el binario cumple
el perfil de porting newlib/c++ estatico para esta fase.

## Archivos del kit

- `crt0.S`: entry `_start` + inicializacion C/C++
- `newlib_syscalls.cpp`: stubs syscall para `newlib` (write/read/openat/brk/exit, etc.)
- `linux_syscall.h`: wrapper `syscall` x86_64
- `build_app.sh`: compilacion + validacion ELF
- `examples/hello_cpp.cpp`: app de ejemplo
