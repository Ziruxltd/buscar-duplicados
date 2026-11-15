// buscar_duplicados - Rust version
// Busca archivos duplicados comparando hashes MD5 en paralelo

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use rayon::prelude::*;
use walkdir::WalkDir;

// Calcula el hash MD5 de un archivo (md5::Context implementa Write).
// BufReader de 1 MB: io::copy lee directamente de su buffer en vez de usar trozos de 8 KB
fn compute_md5(path: &Path) -> Option<String> {
    let mut file = BufReader::with_capacity(1 << 20, File::open(path).ok()?);
    let mut context = md5::Context::new();
    io::copy(&mut file, &mut context).ok()?;
    Some(format!("{:x}", context.compute()))
}

// Archivos de distinto tamaño nunca son duplicados: solo hay que hashear los de tamaño repetido
fn paths_to_hash(files: Vec<(PathBuf, u64)>) -> Vec<PathBuf> {
    let mut by_size: HashMap<u64, Vec<PathBuf>> = HashMap::new();
    for (path, size) in files {
        by_size.entry(size).or_default().push(path);
    }
    by_size.into_values().filter(|v| v.len() > 1).flatten().collect()
}

// Calcula hashes en paralelo de los candidatos y devuelve los grupos con duplicados, ordenados por hash
fn find_duplicate_groups(files: Vec<(PathBuf, u64)>) -> Vec<(String, Vec<PathBuf>)> {
    let results: Vec<(PathBuf, String)> = paths_to_hash(files)
        .into_par_iter()
        .filter_map(|path| match compute_md5(&path) {
            Some(hash) => Some((path, hash)),
            None => {
                eprintln!("  No se pudo leer: {}", path.display());
                None
            }
        })
        .collect();

    let mut hashes: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for (path, hash) in results {
        hashes.entry(hash).or_default().push(path);
    }

    let mut grupos: Vec<(String, Vec<PathBuf>)> =
        hashes.into_iter().filter(|(_, v)| v.len() > 1).collect();
    grupos.sort_by(|a, b| a.0.cmp(&b.0));
    grupos
}

// Detecta si el nombre de archivo (sin extension) termina en patron de copia: " (N)"
fn has_copy_pattern(stem: &str) -> bool {
    let s = stem.trim_end();
    if s.ends_with(')') {
        if let Some(open) = s.rfind('(') {
            let inside = &s[open + 1..s.len() - 1];
            let before = s[..open].trim_end();
            if !before.is_empty() && !inside.is_empty() && inside.chars().all(|c| c.is_ascii_digit()) {
                return true;
            }
        }
    }
    false
}

// Elige que archivo conservar dentro de un grupo de duplicados:
// 1. Prefiere los que NO tienen patron " (N)"
// 2. Entre los candidatos, elige el de nombre mas corto
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
        .min_by_key(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or("").len())
        .unwrap()
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

    // 2. Recopilar todos los archivos recursivamente (excluye archivos de 0 bytes)
    let archivos: Vec<(PathBuf, u64)> = WalkDir::new(&directorio)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| {
            let size = e.metadata().ok()?.len();
            (size > 0).then(|| (e.path().to_path_buf(), size))
        })
        .collect();

    if archivos.is_empty() {
        println!("El directorio no contiene archivos (o todos tienen 0 bytes).");
        return;
    }

    println!("Encontrados {} archivos. Calculando hashes en paralelo...", archivos.len());

    // 3-5. Agrupar por tamaño, hashear solo los de tamaño repetido y agrupar por hash
    let grupos = find_duplicate_groups(archivos);

    if grupos.is_empty() {
        println!("\nNo se encontraron archivos duplicados.");
        return;
    }

    // 6. Generar informe TXT
    let fecha = Local::now().format("%Y-%m-%d_%H-%M-%S").to_string();
    let archivo_salida = directorio.join(format!("duplicados_{}.txt", fecha));

    let mut total_duplicados: usize = 0;
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
            total_duplicados += 1;
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
    println!("\n--- Resumen de acciones ---");
    for (_, rutas) in &grupos {
        let conservar = choose_file_to_keep(rutas);
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

    for (_, rutas) in &grupos {
        let conservar = choose_file_to_keep(rutas);
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

    // Directorio temporal propio para cada test; el test lo borra al terminar
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("buscar_duplicados_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn compute_md5_de_contenido_conocido() {
        let dir = temp_dir("md5");
        let f = dir.join("hello.txt");
        std::fs::write(&f, b"hello").unwrap();
        let hash = compute_md5(&f);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(hash.as_deref(), Some("5d41402abc4b2a76b9719d911017c592"));
    }

    #[test]
    fn tamanos_unicos_no_se_hashea_nada() {
        let files = vec![(PathBuf::from("a"), 1), (PathBuf::from("b"), 2)];
        assert!(paths_to_hash(files).is_empty());
    }

    #[test]
    fn solo_se_hashean_los_de_tamano_repetido() {
        let files = vec![
            (PathBuf::from("a"), 10),
            (PathBuf::from("b"), 20),
            (PathBuf::from("c"), 10),
        ];
        let mut got = paths_to_hash(files);
        got.sort();
        assert_eq!(got, vec![PathBuf::from("a"), PathBuf::from("c")]);
    }

    #[test]
    fn grupos_duplicados_con_archivos_reales() {
        let dir = temp_dir("grupos");
        let contenidos: [(&str, &[u8]); 4] = [
            ("uno.jpg", b"abcd"),
            ("uno (1).jpg", b"abcd"),
            ("mismo_tamano.jpg", b"wxyz"),
            ("otro_tamano.jpg", b"abcdef"),
        ];
        let files: Vec<(PathBuf, u64)> = contenidos
            .iter()
            .map(|(name, data)| {
                let p = dir.join(name);
                std::fs::write(&p, data).unwrap();
                (p, data.len() as u64)
            })
            .collect();
        let grupos = find_duplicate_groups(files);
        std::fs::remove_dir_all(&dir).unwrap();

        assert_eq!(grupos.len(), 1);
        let mut rutas = grupos[0].1.clone();
        rutas.sort();
        assert_eq!(rutas, vec![dir.join("uno (1).jpg"), dir.join("uno.jpg")]);
    }
}
