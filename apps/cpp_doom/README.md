# CPP-DOOM App (Go OS)

Este folder empaqueta `cpp-doom` para instalarlo en Go OS y lanzarlo desde GUI.

## 1) Compilar binario Linux ELF64

```bash
apps/cpp_doom/build.sh
```

Salida esperada: `apps/cpp_doom/CPPDOOM.BIN`

## 2) Empaquetar RPX

```bash
ruby tools/redux_recipe_build.rb recipes/cpp_doom/recipe.toml
```

Salida esperada: `packages/cpp_doom.rpx`

## 3) Instalar en Go OS

En terminal GUI de Go OS:

```text
install cpp_doom.rpx CPPDOOM
```

Luego puedes abrirlo con:

```text
cppdoom
```

O desde Start -> Games -> CPP-DOOM Launcher.
