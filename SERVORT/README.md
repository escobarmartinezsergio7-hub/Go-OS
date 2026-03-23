# ServoRT Runtime (sin host externo)

Este directorio contiene el ejecutable LinuxRT que el backend `web backend servort` lanza en Go OS.

Ruta esperada por kernel:

```
/SERVORT/SVRT0001.BIN
```

## Stage rapido

Desde la raiz del repo:

```bash
make servort-stage SERVO_BIN=/Users/mac/Desktop/servo/target/release/servo
```

Si quieres copiar directo al ESP local de build:

```bash
make servort-stage-esp SERVO_BIN=/Users/mac/Desktop/servo/target/release/servo
```

## Notas importantes

- `SVRT0001.BIN` debe ser **Linux ELF x86_64**.
- Binarios macOS (`Mach-O`) o Windows (`PE`) no corren en LinuxRT.
- Asegura que sus dependencias dinamicas esten disponibles en `/LINUXRT`.
