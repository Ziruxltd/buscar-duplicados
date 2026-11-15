# buscar_duplicados.ps1
# Busca archivos duplicados en un directorio comparando hashes MD5

while ($true) {
    $directorio = Read-Host "`nIntroduce la ruta del directorio"

    if (Test-Path -Path $directorio -PathType Container) {
        break
    } else {
        Write-Host "ERROR: El directorio '$directorio' no existe. Intentalo de nuevo." -ForegroundColor Red
    }
}

Write-Host "`nAnalizando archivos en: $directorio" -ForegroundColor Cyan
Write-Host "Calculando hashes (puede tardar en directorios grandes)..." -ForegroundColor Cyan

# Obtener todos los archivos recursivamente y calcular su hash MD5
$archivos = Get-ChildItem -Path $directorio -Recurse -File

if ($archivos.Count -eq 0) {
    Write-Host "El directorio no contiene archivos." -ForegroundColor Yellow
    exit
}

$hashes = @{}

foreach ($archivo in $archivos) {
    try {
        $hash = (Get-FileHash -Path $archivo.FullName -Algorithm MD5).Hash
        if ($hashes.ContainsKey($hash)) {
            $hashes[$hash] += @($archivo.FullName)
        } else {
            $hashes[$hash] = @($archivo.FullName)
        }
    } catch {
        Write-Host "No se pudo leer: $($archivo.FullName)" -ForegroundColor DarkYellow
    }
}

# Filtrar solo los grupos con mas de un archivo (duplicados)
$duplicados = $hashes.GetEnumerator() | Where-Object { $_.Value.Count -gt 1 }

if (-not $duplicados) {
    Write-Host "`nNo se encontraron archivos duplicados." -ForegroundColor Green
    exit
}

# Generar nombre del archivo de salida con fecha y hora
$fecha = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"
$archivoSalida = Join-Path $directorio "duplicados_$fecha.txt"

$lineas = @()
$lineas += "INFORME DE ARCHIVOS DUPLICADOS"
$lineas += "Directorio analizado: $directorio"
$lineas += "Fecha de analisis: $(Get-Date -Format 'dd/MM/yyyy HH:mm:ss')"
$lineas += "=" * 60
$lineas += ""

$totalDuplicados = 0
$grupo = 1

foreach ($entrada in $duplicados) {
    $lineas += "--- Grupo $grupo (Hash MD5: $($entrada.Key)) ---"
    foreach ($ruta in $entrada.Value) {
        $lineas += "  $ruta"
        $totalDuplicados++
    }
    $lineas += ""
    $grupo++
}

$lineas += "=" * 60
$lineas += "Total de grupos duplicados: $($grupo - 1)"
$lineas += "Total de archivos duplicados: $totalDuplicados"

$lineas | Out-File -FilePath $archivoSalida -Encoding UTF8

Write-Host "`nSe encontraron $($grupo - 1) grupos de archivos duplicados." -ForegroundColor Yellow
Write-Host "Informe guardado en: $archivoSalida" -ForegroundColor Green

# Preguntar si se desean borrar los duplicados
Write-Host ""
$respuesta = Read-Host "Se han encontrado $totalDuplicados archivos duplicados. ¿Quieres que los borremos? (s/n)"

if ($respuesta -notmatch '^[sS]$') {
    Write-Host "Operacion cancelada. No se ha borrado ningun archivo." -ForegroundColor Cyan
    exit
}

# Funcion para elegir el archivo a conservar dentro de un grupo:
# Se conserva el que NO tenga patron " (N)" en el nombre y, en caso de empate, el de nombre mas corto.
function Get-ArchivoAConservar {
    param([string[]]$rutas)

    # Separar los que tienen patron de copia " (1)", " (2)", etc. en el nombre base (sin extension)
    $sinCopia = $rutas | Where-Object {
        [System.IO.Path]::GetFileNameWithoutExtension($_) -notmatch '\s*\(\d+\)\s*$'
    }
    $conCopia = $rutas | Where-Object {
        [System.IO.Path]::GetFileNameWithoutExtension($_) -match '\s*\(\d+\)\s*$'
    }

    # Candidatos a conservar: primero los sin patron de copia; si todos lo tienen, todos son candidatos
    $candidatos = if ($sinCopia) { $sinCopia } else { $rutas }

    # De los candidatos, conservar el de nombre de archivo mas corto (solo nombre, sin ruta)
    $conservar = $candidatos | Sort-Object { [System.IO.Path]::GetFileName($_).Length } | Select-Object -First 1
    return $conservar
}

Write-Host "`nBorrando duplicados..." -ForegroundColor Cyan

$borrados = 0
$errores = 0

foreach ($entrada in $duplicados) {
    $rutas = $entrada.Value
    $conservar = Get-ArchivoAConservar -rutas $rutas
    $aBorrar = $rutas | Where-Object { $_ -ne $conservar }

    foreach ($ruta in $aBorrar) {
        try {
            $shell = New-Object -ComObject Shell.Application
            $carpeta = $shell.Namespace([System.IO.Path]::GetDirectoryName($ruta))
            $item = $carpeta.ParseName([System.IO.Path]::GetFileName($ruta))
            $item.InvokeVerb("delete")
            Write-Host "  Enviado a papelera: $ruta" -ForegroundColor DarkGray
            $borrados++
        } catch {
            Write-Host "  ERROR al mover a papelera: $ruta" -ForegroundColor Red
            $errores++
        }
    }
    Write-Host "  Conservado: $conservar" -ForegroundColor Green
}

Write-Host ""
Write-Host "Proceso completado." -ForegroundColor Cyan
Write-Host "  Archivos borrados : $borrados" -ForegroundColor Yellow
if ($errores -gt 0) {
    Write-Host "  Errores al borrar : $errores" -ForegroundColor Red
}
