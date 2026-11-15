# Roadmap: buscar-duplicados

Objetivo: una herramienta de consola en Rust, instalable, que encuentre fotos y vídeos
duplicados y mande las copias a la papelera, rápida incluso con cientos de GB.

Uso final previsto (el nombre `dupli` es provisional, se cambia en un sitio):

```powershell
cd D:\Fotos
dupli buscar            # analiza la carpeta actual y escribe el informe
dupli buscar D:\Viaje   # analiza la carpeta indicada
dupli limpiar           # informe + resumen + confirmación + papelera
dupli                   # muestra la ayuda
```

Los números entre corchetes, como [A1], sirven para pedir "haz el A1".

---

## Fase 0: que compile (bloquea todo lo demás)

- **[0] Arreglar la compilación en este PC.** Hoy `cargo build` falla: la carpeta fuerza
  la variante GNU de Rust (le falta `dlltool.exe`), y la variante MSVC no encuentra un
  `link.exe` válido. Solución probable: quitar el override (`rustup override unset`) e
  instalar "Build Tools de Visual Studio" (C++). Listo cuando pasen `cargo build --release`
  y `cargo test`.

Sin esto no se puede probar nada de lo que viene después.

## Fase A: bugs que pueden borrar lo que no toca

| #  | Qué | Tamaño | Depende de |
|----|-----|--------|------------|
| A1 | Test de la regla de "qué archivo conservar" (`foto.jpg` vs `foto (1).jpg`, todos con `(N)`, `Boda (2023).jpg`) | ~25 líneas | 0 |
| A2 | Mostrar bien cuántos archivos se van a borrar (`rutas.len() - 1` por grupo) | 1 línea | 0 |
| A3 | Analizar solo fotos y vídeos (filtro por extensión), así no toca `desktop.ini` ni otros archivos | ~5 líneas | 0 |
| A4 | Desempate al elegir el archivo a conservar: el más antiguo o la ruta más corta. Arreglar también `Boda (2023).jpg` | ~3 líneas | **A1** (el test protege el cambio) |
| A5 | Borrar `buscar_duplicados.ps1` (versión vieja que borra archivos vacíos) | -142 líneas | nada |

## Fase B: eficiencia (la razón de que sea Rust)

| #  | Qué | Tamaño | Depende de |
|----|-----|--------|------------|
| B1 | Agrupar por tamaño y hacer el hash solo de los archivos cuyo tamaño se repite | ~10 líneas | **A3** (mismas líneas: recogida de archivos) |
| B2 | Simplificar la lectura: `io::copy` al `md5::Context` en lugar de dos buffers | -8 líneas | nada (se puede hacer junto con B1) |
| B3 | Medir en disco duro externo vs SSD; si el paralelo va peor, limitar los hilos | medir + ~2 líneas | **B1** (medir sobre la versión rápida) |

Mejora opcional para después de B1: comparar primero los primeros 64 KB antes del hash
completo. Solo merece la pena si B1 no es suficiente con vídeos grandes.

## Fase C: programa instalable con comandos

| #  | Qué | Tamaño | Depende de |
|----|-----|--------|------------|
| C1 | Leer comando y ruta de los argumentos (`std::env::args`, sin librerías). Sin ruta, usar la carpeta actual (`std::env::current_dir`). Quitar la pregunta "Introduce la ruta" | ~30 líneas | A2, A3 (tocan el mismo `main`) |
| C2 | Separar `buscar` (solo informe) de `limpiar` (informe + confirmación + papelera). Sin comando o con comando desconocido: ayuda breve | incluido en C1 | C1 |
| C3 | Nombre del ejecutable: `[[bin]] name = "dupli"` en `Cargo.toml` | 2 líneas | nada |
| C4 | Instalación: `cargo install --path .` lo copia a `%USERPROFILE%\.cargo\bin`, que ya está en el PATH, y funciona desde cualquier PowerShell. Documentarlo en un README corto | README | **0**, C1, C3 |
| C5 | (Más adelante) Distribuir un `.exe` para PCs sin Rust: descarga desde GitHub Releases o paquete de `winget` | aparte | C4 |

---

## Orden y paralelismo

```text
[0] compilar
 ├─► A1 test ──► A4 desempate
 ├─► A2 contador ─┐
 ├─► A3 filtro ───┼─► B1 por tamaño ─► B3 medir disco
 │                └─► C1 argumentos ─► C2 buscar/limpiar ─► C4 instalar
 │       B2 io::copy (cuando sea, mejor junto a B1)
 └─  A5 borrar .ps1 y C3 nombre: en cualquier momento, ni siquiera necesitan [0]
```

**Secuenciales (uno depende de otro):**
- 0 va antes que todo lo que haya que probar.
- A1 → A4: primero el test y luego el cambio de regla.
- A3 → B1: tocan las mismas líneas (la recogida de archivos).
- B1 → B3: se mide la versión ya optimizada.
- A2 y A3 → C1 → C2 → C4: C1 reescribe `main`, mejor sobre el código ya arreglado.

**Independientes (orden libre):** A1, A2, A3, A5, B2 y C3 no dependen entre sí.

**Ojo con "a la vez de verdad":** casi todo vive en `src/main.rs`, así que dos cambios
simultáneos en sesiones distintas chocarían. A la vez de verdad solo se pueden hacer A5
(otro archivo), C3 (`Cargo.toml`) y A1 si el test va en su propio módulo. Lo demás conviene
hacerlo de uno en uno; cada uno es pequeño.

## Orden recomendado

1. **0**: compilar.
2. **A5 + C3**: limpieza rápida, sin riesgo.
3. **A1 → A2 → A3 → A4**: que no borre lo que no debe.
4. **B1 + B2**, luego **B3**: que vaya rápido.
5. **C1 → C2 → C4**: instalable y con comandos.
6. **C5**, si quieres repartirlo a otros PCs.
