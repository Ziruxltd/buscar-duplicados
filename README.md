# dupli

Busca fotos y vídeos duplicados en una carpeta y sus subcarpetas, y manda las copias a la
Papelera de reciclaje. Conserva siempre un archivo de cada grupo.

Escrito en Rust para que sea rápido con bibliotecas grandes: solo calcula el hash de los
archivos cuyo tamaño se repite (dos archivos de distinto tamaño nunca son iguales) y lo hace
en paralelo.

## Instalación

### Requisitos (Windows)

- Rust instalado con [rustup](https://rustup.rs), con la toolchain MSVC (la de por defecto).
- Visual Studio o Build Tools for Visual Studio con la carga de trabajo
  "Desarrollo para el escritorio con C++", incluido el SDK de Windows 11.

### Instalar

```powershell
git clone https://github.com/Ziruxltd/buscar-duplicados.git
cd buscar-duplicados
cargo install --path .
```

Esto deja `dupli.exe` en `%USERPROFILE%\.cargo\bin`, que rustup ya añade al PATH.
Abre una PowerShell nueva y ya puedes usar `dupli` en cualquier carpeta.

### Actualizar

Desde la carpeta del proyecto:

```powershell
git pull
cargo install --path .
```

### Desinstalar

```powershell
cargo uninstall buscar-duplicados
```

## Uso

```powershell
cd D:\Fotos
dupli buscar            # analiza la carpeta actual y guarda el informe; no borra nada
dupli buscar D:\Viaje   # analiza la carpeta indicada
dupli limpiar           # informe + resumen + doble confirmación + papelera
dupli limpiar D:\Viaje
dupli ayuda             # muestra la ayuda (también -h o --help)
```

Sin ruta, `dupli` usa la carpeta en la que está la PowerShell.

### Qué hace cada comando

- **`buscar`** escribe un informe `duplicados_<fecha>.txt` en la carpeta analizada con
  cada grupo de archivos idénticos. No toca ningún archivo.
- **`limpiar`** hace lo mismo, te enseña qué archivo se conserva y cuáles van a la
  papelera, y pide confirmación dos veces antes de moverlos. Van a la Papelera de
  reciclaje, así que puedes recuperarlos.

### Qué archivos analiza

Solo fotos y vídeos, por extensión: `jpg`, `jpeg`, `png`, `gif`, `bmp`, `tif`, `tiff`,
`webp`, `heic`, `heif`, `avif`, RAW (`raw`, `cr2`, `nef`, `arw`, `dng`, `raf`, `orf`, `rw2`)
y vídeo (`mp4`, `mov`, `avi`, `mkv`, `m4v`, `3gp`, `wmv`, `mts`, `mpg`, `mpeg`, `webm`).
Ignora los archivos vacíos y los del sistema como `desktop.ini`.

Dos archivos son duplicados si su contenido es idéntico byte a byte (mismo hash MD5).
Una foto redimensionada o recomprimida no cuenta como duplicada.

### Qué archivo se conserva

En cada grupo de duplicados:

1. Se prefieren los que no tienen sufijo de copia de Windows, como `foto (1).jpg`.
   Un año entre paréntesis, como `Boda (2023).jpg`, no cuenta como copia.
2. Entre esos, el de nombre más corto.
3. Si hay empate, el de fecha de modificación más antigua y, después, el de ruta
   alfabéticamente primera.

## Desarrollo

```powershell
cargo test      # tests
cargo clippy    # revisión de estilo y errores comunes
```

## Licencia

[MIT](LICENSE): cualquiera puede usar, copiar, modificar y distribuir este software.
