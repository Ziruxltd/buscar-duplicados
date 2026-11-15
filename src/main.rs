// buscar_duplicados - Rust version
// Busca archivos duplicados comparando hashes MD5 en paralelo

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use rayon::prelude::*;
use walkdir::WalkDir;

// Calcula el hash MD5 de un archivo usando un buffer grande para I/O eficiente
fn compute_md5(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut context = md5::Context::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = reader.read(&mut buffer).ok()?;
        if n == 0 {
            break;
        }
        context.consume(&buffer[..n]);
    }
    Some(format!("{:x}", context.compute()))
}

const MEDIA_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tif", "tiff", "webp", "heic", "heif", "raw", "cr2", "nef",
    "arw", "dng", "avif", "raf", "orf", "rw2", "mp4", "mov", "avi", "mkv", "m4v", "3gp", "wmv",
    "mts", "mpg", "mpeg", "webm",
];

// Solo fotos y videos: evita borrar desktop.ini, Thumbs.db, etc.
fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| MEDIA_EXTENSIONS.iter().any(|m| e.eq_ignore_ascii_case(m)))
}

// Detecta si el nombre de archivo (sin extension) termina en patron de copia: " (N)"
fn has_copy_pattern(stem: &str) -> bool {
    let s = stem.trim_end();
    if s.ends_with(')') {
        if let Some(open) = s.rfind('(') {
            let inside = &s[open + 1..s.len() - 1];
            let before = s[..open].trim_end();
            // 1-3 digitos como Windows " (1)"; 4 digitos suele ser un año: "Boda (2023)"
            if !before.is_empty()
                && (1..=3).contains(&inside.len())
                && inside.chars().all(|c| c.is_ascii_digit())
            {
                return true;
            }
        }
    }
    false
}

// Elige que archivo conservar dentro de un grupo de duplicados:
// 1. Prefiere los que NO tienen patron " (N)"
// 2. Entre los candidatos, elige el de nombre mas corto
// 3. Empate: el de fecha de modificacion mas antigua, y luego la ruta alfabetica
fn choose_file_to_keep(paths: &[PathBuf]) -> &PathBuf {
    let without_copy: Vec<&PathBuf> = paths
        .iter()
        .filter(|p| {
            let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            !has_copy_pattern(stem)
        })
        .collect();

    let candidates: Vec<&PathBuf> = if without_copy.is_empty() {
        paths.iter().collect()
    } else {
        without_copy
    };

    candidates
        .into_iter()
        .min_by_key(|p| {
            let len = p.file_name().and_then(|n| n.to_str()).unwrap_or("").len();
            let modified = std::fs::metadata(p).and_then(|m| m.modified()).ok();
            // fecha ilegible al final: None < Some haria conservar un archivo que quiza ya no existe
            (len, modified.is_none(), modified, *p)
        })
        .unwrap()
}

// Archivos que irian a la papelera: todos menos el conservado de cada grupo
fn count_to_trash(grupos: &[(String, Vec<PathBuf>)]) -> usize {
    grupos.iter().map(|(_, v)| v.len() - 1).sum()
}

fn read_line() -> String {
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    input.trim().to_string()
}

fn main() {
    // 1. Pedir directorio en bucle hasta que exista
    let directorio: PathBuf = loop {
        print!("\nIntroduce la ruta del directorio: ");
        io::stdout().flush().unwrap();
        let input = read_line();
        let path = PathBuf::from(&input);
        if path.is_dir() {
            break path;
        } else {
            eprintln!("ERROR: El directorio '{}' no existe. Intentalo de nuevo.", input);
        }
    };

    println!("\nAnalizando archivos en: {}", directorio.display());
    println!("Calculando hashes en paralelo (puede tardar en directorios grandes)...");

    // 2. Recopilar fotos y videos recursivamente (excluye archivos de 0 bytes)
    let archivos: Vec<PathBuf> = WalkDir::new(&directorio)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && is_media(e.path())
                && e.metadata().map(|m| m.len() > 0).unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect();

    if archivos.is_empty() {
        println!("El directorio no contiene fotos ni videos (o todos tienen 0 bytes).");
        return;
    }

    println!("Encontrados {} archivos. Calculando hashes en paralelo...", archivos.len());

    // 3. Calcular hashes en paralelo con rayon
    let results: Vec<(PathBuf, String)> = archivos
        .par_iter()
        .filter_map(|path| match compute_md5(path) {
            Some(hash) => Some((path.clone(), hash)),
            None => {
                eprintln!("  No se pudo leer: {}", path.display());
                None
            }
        })
        .collect();

    // 4. Agrupar por hash
    let mut hashes: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (path, hash) in results {
        hashes.entry(hash).or_default().push(path);
    }

    // 5. Filtrar solo grupos con duplicados
    let mut grupos: Vec<(String, Vec<PathBuf>)> = hashes
        .into_iter()
        .filter(|(_, v)| v.len() > 1)
        .collect();

    if grupos.is_empty() {
        println!("\nNo se encontraron archivos duplicados.");
        return;
    }

    // Ordenar para salida determinista
    grupos.sort_by(|a, b| a.0.cmp(&b.0));

    // 6. Generar informe TXT
    let fecha = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let archivo_salida = directorio.join(format!("duplicados_{}.txt", fecha));

    let total_duplicados = count_to_trash(&grupos);
    let mut lineas: Vec<String> = Vec::new();
    lineas.push("INFORME DE ARCHIVOS DUPLICADOS".to_string());
    lineas.push(format!("Directorio analizado: {}", directorio.display()));
    lineas.push(format!("Fecha de analisis: {}", Local::now().format("%d/%m/%Y %H:%M:%S")));
    lineas.push("=".repeat(60));
    lineas.push(String::new());

    for (i, (hash, rutas)) in grupos.iter().enumerate() {
        lineas.push(format!("--- Grupo {} (Hash MD5: {}) ---", i + 1, hash));
        for ruta in rutas {
            lineas.push(format!("  {}", ruta.display()));
        }
        lineas.push(String::new());
    }

    lineas.push("=".repeat(60));
    lineas.push(format!("Total de grupos duplicados: {}", grupos.len()));
    lineas.push(format!("Total de archivos duplicados: {}", total_duplicados));

    std::fs::write(&archivo_salida, lineas.join("\r\n").as_bytes())
        .expect("No se pudo escribir el informe");

    println!("\nSe encontraron {} grupos de archivos duplicados.", grupos.len());
    println!("Informe guardado en: {}", archivo_salida.display());

    // 7. Preguntar si borrar
    println!();
    print!(
        "Se han encontrado {} archivos duplicados. ¿Quieres que los borremos? (s/n): ",
        total_duplicados
    );
    io::stdout().flush().unwrap();
    let respuesta = read_line();

    if respuesta.to_lowercase() != "s" {
        println!("Operacion cancelada. No se ha borrado ningun archivo.");
        return;
    }

    // 8. Mostrar resumen de lo que se va a hacer antes de borrar
    // Elegir una sola vez: el borrado debe hacer exactamente lo que el usuario confirma
    let conservados: Vec<&PathBuf> = grupos.iter().map(|(_, r)| choose_file_to_keep(r)).collect();

    println!("\n--- Resumen de acciones ---");
    for ((_, rutas), &conservar) in grupos.iter().zip(&conservados) {
        for ruta in rutas {
            if ruta == conservar {
                println!("  [CONSERVAR] {}", ruta.display());
            } else {
                println!("  [PAPELERA]  {}", ruta.display());
            }
        }
        println!();
    }

    print!("¿Confirmas el borrado? (s/n): ");
    io::stdout().flush().unwrap();
    let confirmacion = read_line();

    if confirmacion.to_lowercase() != "s" {
        println!("Operacion cancelada. No se ha borrado ningun archivo.");
        return;
    }

    // 9. Mover a papelera
    println!("\nMoviendo duplicados a la papelera...");
    let mut borrados: usize = 0;
    let mut errores: usize = 0;

    for ((_, rutas), &conservar) in grupos.iter().zip(&conservados) {
        for ruta in rutas {
            if ruta == conservar {
                continue;
            }
            match trash::delete(ruta) {
                Ok(_) => {
                    println!("  Enviado a papelera: {}", ruta.display());
                    borrados += 1;
                }
                Err(e) => {
                    eprintln!("  ERROR al mover a papelera: {} ({})", ruta.display(), e);
                    errores += 1;
                }
            }
        }
        println!("  Conservado:         {}", conservar.display());
    }

    println!();
    println!("Proceso completado.");
    println!("  Archivos enviados a papelera : {}", borrados);
    if errores > 0 {
        println!("  Errores                      : {}", errores);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<PathBuf> {
        names.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn copy_pattern_detects_windows_copy_suffix() {
        assert!(has_copy_pattern("foto (1)"));
        assert!(has_copy_pattern("foto (12)"));
        assert!(!has_copy_pattern("foto"));
        assert!(!has_copy_pattern("(1)"));
    }

    #[test]
    fn keeps_original_over_copy() {
        let p = paths(&["foto (1).jpg", "foto.jpg"]);
        assert_eq!(choose_file_to_keep(&p), &PathBuf::from("foto.jpg"));
    }

    #[test]
    fn all_copies_keeps_shortest_name() {
        let p = paths(&["foto (12).jpg", "foto (1).jpg"]);
        assert_eq!(choose_file_to_keep(&p), &PathBuf::from("foto (1).jpg"));
    }

    #[test]
    fn count_to_trash_excludes_kept_file() {
        let grupos = vec![
            ("a".to_string(), paths(&["x.jpg", "x (1).jpg"])),
            ("b".to_string(), paths(&["y.jpg", "y (1).jpg", "y (2).jpg"])),
        ];
        assert_eq!(count_to_trash(&grupos), 3);
    }

    #[test]
    fn only_photos_and_videos_are_media() {
        assert!(is_media(Path::new("C:\\fotos\\IMG.JPG")));
        assert!(is_media(Path::new("video.Mov")));
        assert!(is_media(Path::new("raw.dng")));
        for ext in ["mpg", "mpeg", "webm", "avif", "raf", "orf", "rw2"] {
            assert!(is_media(Path::new(&format!("a.{}", ext))), "{}", ext);
        }
        assert!(!is_media(Path::new("desktop.ini")));
        assert!(!is_media(Path::new("Thumbs.db")));
        assert!(!is_media(Path::new("sin_extension")));
    }

    #[test]
    fn year_in_parentheses_is_not_copy() {
        assert!(!has_copy_pattern("Boda (2023)"));
        assert!(has_copy_pattern("Boda (123)"));
    }

    #[test]
    fn year_named_original_kept_over_copy() {
        let p = paths(&["Boda (2023) (1).jpg", "Boda (2023).jpg"]);
        assert_eq!(choose_file_to_keep(&p), &PathBuf::from("Boda (2023).jpg"));
    }

    // Crea los archivos en un directorio temporal con la fecha de modificacion indicada (segundos)
    fn make_files(test: &str, files: &[(&str, u64)]) -> (PathBuf, Vec<PathBuf>) {
        let root = std::env::temp_dir().join(format!("buscar_dup_{}_{}", test, std::process::id()));
        let mut out = Vec::new();
        for (rel, secs) in files {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let f = File::create(&p).unwrap();
            f.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(*secs)).unwrap();
            out.push(p);
        }
        (root, out)
    }

    #[test]
    fn same_name_length_keeps_oldest_modified() {
        let (root, p) = make_files(
            "mtime",
            &[("Descargas/IMG_1.jpg", 2_000_000_000), ("Viaje Roma/IMG_1.jpg", 1_000_000_000)],
        );
        let kept = choose_file_to_keep(&p).clone();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(kept, p[1]);
    }

    #[test]
    fn same_name_and_mtime_keeps_alphabetical_path() {
        let (root, p) = make_files(
            "path",
            &[("Viaje Roma/IMG_1.jpg", 1_000_000_000), ("Descargas/IMG_1.jpg", 1_000_000_000)],
        );
        let kept = choose_file_to_keep(&p).clone();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(kept, p[1]);
    }

    #[test]
    fn unreadable_mtime_never_wins_tie_break() {
        let (root, mut p) = make_files("missing", &[("B/IMG_1.jpg", 1_000_000_000)]);
        p.insert(0, root.join("A/IMG_1.jpg")); // no existe: alfabeticamente primero
        let kept = choose_file_to_keep(&p).clone();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(kept, p[1]);
    }
}
