# dupli

Busca fotos y vídeos duplicados en una carpeta (y sus subcarpetas) comparando su hash MD5.
Guarda un informe `duplicados_<fecha>.txt` en la carpeta analizada y, si se lo pides,
envía las copias a la Papelera de reciclaje (conserva un archivo de cada grupo).

## Instalación

Requisitos:

- Rust instalado con [rustup](https://rustup.rs), con la toolchain MSVC por defecto.
- Visual Studio (o Build Tools for Visual Studio) con la carga de trabajo
  "Desarrollo para el escritorio con C++", incluido el SDK de Windows 11.

Desde la carpeta del proyecto:

```powershell
cargo install --path .
```

Esto deja `dupli.exe` en `%USERPROFILE%\.cargo\bin`, que rustup ya añade al PATH.
Abre una PowerShell nueva y ya puedes usar `dupli` en cualquier carpeta.

## Uso

```powershell
cd D:\Fotos
dupli buscar            # analiza la carpeta actual y guarda el informe, no borra nada
dupli buscar D:\Fotos   # analiza esa carpeta
dupli limpiar           # informe + resumen + doble confirmación + papelera
dupli limpiar D:\Fotos
dupli                   # muestra la ayuda
```

## Actualizar

Tras cambiar el código, repite el mismo comando:

```powershell
cargo install --path .
```

## Desinstalar

```powershell
cargo uninstall buscar-duplicados
```
