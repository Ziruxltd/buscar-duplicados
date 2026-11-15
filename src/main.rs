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
    // 1. Leer comando y directorio de los argumentos
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let Comando { accion, directorio } = match parse_args(&args, &cwd)
        .and_then(|c| comprobar_directorio(&c.directorio).map(|_| c))
    {
        Ok(c) if c.accion == Accion::Ayuda => {
            println!("{}", AYUDA);
            return;
        }
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    println!("\nAnalizando archivos en: {}", directorio.display());
    println!("Calculando hashes en paralelo (puede tardar en directorios grandes)...");

    // 2. Recopilar fotos y videos recursivamente con su tamaño (excluye archivos de 0 bytes)
    let archivos: Vec<(PathBuf, u64)> = WalkDir::new(&directorio)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_media(e.path()))
        .filter_map(|e| {
            let size = e.metadata().ok()?.len();
            (size > 0).then(|| (e.path().to_path_buf(), size))
        })
        .collect();

    if archivos.is_empty() {
        println!("El directorio no contiene fotos ni videos (o todos tienen 0 bytes).");
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

    if accion == Accion::Buscar {
        return;
    }

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

#[derive(Debug, PartialEq)]
enum Accion {
    Buscar,
    Limpiar,
    Ayuda,
}

#[derive(Debug, PartialEq)]
struct Comando {
    accion: Accion,
    directorio: PathBuf,
}

const AYUDA: &str = "Uso: dupli <comando> [ruta]

Comandos:
  buscar [ruta]    Busca duplicados y guarda el informe duplicados_*.txt (no borra nada)
  limpiar [ruta]   Busca duplicados, guarda el informe y, tras confirmar, los envia a la papelera

  ayuda, -h, --help  Muestra esta ayuda

Sin ruta se usa la carpeta actual.";

// args: los argumentos sin el nombre del programa
fn parse_args(args: &[String], cwd: &Path) -> Result<Comando, String> {
    let accion = match args.first().map(String::as_str) {
        None => return Err(AYUDA.to_string()),
        Some("buscar") => Accion::Buscar,
        Some("limpiar") => Accion::Limpiar,
        Some("--help" | "-h" | "ayuda") => Accion::Ayuda,
        Some(otro) => return Err(format!("ERROR: comando desconocido '{}'.\n\n{}", otro, AYUDA)),
    };
    let directorio = match &args[1..] {
        [] => cwd.to_path_buf(),
        // PowerShell 5.1 pasa '.\Mis Fotos\' como ".\Mis Fotos\" y Windows convierte \" en una comilla literal
        [ruta] => PathBuf::from(ruta.trim_end_matches('"')),
        _ => return Err(format!("ERROR: demasiados argumentos.\n\n{}", AYUDA)),
    };
    Ok(Comando { accion, directorio })
}

fn comprobar_directorio(dir: &Path) -> Result<(), String> {
    if dir.is_dir() {
        Ok(())
    } else {
        Err(format!("ERROR: '{}' no existe o no es un directorio.", dir.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn buscar_sin_ruta_usa_cwd() {
        let cwd = Path::new(r"C:\actual");
        assert_eq!(
            parse_args(&args(&["buscar"]), cwd),
            Ok(Comando { accion: Accion::Buscar, directorio: cwd.to_path_buf() })
        );
    }

    #[test]
    fn limpiar_con_ruta_usa_esa_ruta() {
        assert_eq!(
            parse_args(&args(&["limpiar", r"D:\x"]), Path::new(r"C:\actual")),
            Ok(Comando { accion: Accion::Limpiar, directorio: PathBuf::from(r"D:\x") })
        );
    }

    #[test]
    fn sin_argumentos_devuelve_ayuda() {
        assert_eq!(parse_args(&[], Path::new(".")), Err(AYUDA.to_string()));
    }

    #[test]
    fn comando_desconocido_es_error() {
        let err = parse_args(&args(&["borrar"]), Path::new(".")).unwrap_err();
        assert!(err.contains("borrar") && err.contains(AYUDA));
    }

    #[test]
    fn demasiados_argumentos_es_error() {
        let err = parse_args(&args(&["buscar", "a", "b"]), Path::new(".")).unwrap_err();
        assert!(err.contains(AYUDA));
    }

    #[test]
    fn ruta_con_comilla_final_de_powershell_se_limpia() {
        let c = parse_args(&args(&["buscar", r#"D:\Mis Fotos""#]), Path::new(".")).unwrap();
        assert_eq!(c.directorio, PathBuf::from(r"D:\Mis Fotos"));
    }

    #[test]
    fn opciones_de_ayuda_devuelven_accion_ayuda() {
        for op in ["--help", "-h", "ayuda"] {
            assert_eq!(parse_args(&args(&[op]), Path::new(".")).map(|c| c.accion), Ok(Accion::Ayuda), "{}", op);
        }
    }

    #[test]
    fn directorio_existente_es_valido() {
        assert_eq!(comprobar_directorio(&std::env::temp_dir()), Ok(()));
    }

    #[test]
    fn directorio_inexistente_es_error() {
        let err = comprobar_directorio(Path::new(r"Z:\no\existe\dupli")).unwrap_err();
        assert!(err.contains(r"Z:\no\existe\dupli"));
    }

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
