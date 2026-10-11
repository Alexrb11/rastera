# Preparar y verificar una alpha

La release pública actual es `0.1.0-alpha.2` para Windows. Este procedimiento prepara `0.1.0-alpha.3` con cuatro paquetes predeterminados: Windows MSVC, Linux x64, macOS arm64 y macOS x64. Los paquetes experimentales necesitan revisión de resultados y pruebas de escritorio; que exista un artefacto no significa que esté publicado ni validado para todos los usuarios.

## Preparación común

Usa un checkout limpio, Rust 1.95 o posterior, Python 3.11 o posterior, Git y el componente `rust-docs`. La versión del workspace, el lockfile, el tag y `docs/releases/<version>.md` deben coincidir. Los empaquetadores compilan el binario normal con `--locked` y sin `capture`; rechazan una build de captura, un target equivocado, avisos de licencia ausentes y salidas ya existentes. Usa otra carpeta de salida para repetir una prueba.

Los paquetes conservan target, commit, compilador y SHA-256 del ejecutable en `BUILD-INFO.json`, junto con avisos de dependencias y biblioteca estándar de Rust, logo, iconos y tutorial. La huella verifica integridad y no sustituye una firma del editor. Cada plataforma debe compilarse y empaquetarse en un host de ese sistema; no se promete que una compilación cruzada sea equivalente.

Antes de crear el tag:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

## Windows

MSVC necesita las herramientas de C++ y el SDK de Windows. GNU también está admitido mediante `-Target x86_64-pc-windows-gnu`, con MinGW compatible; no forma parte de los cuatro paquetes predeterminados de esta entrega.

```powershell
pwsh -File scripts/package-windows.ps1 -Target x86_64-pc-windows-msvc
pwsh -File scripts/verify-portable.ps1 -Archive work/releases/rastera-0.1.0-alpha.3-windows-x64-msvc.zip -ReportPath work/releases/windows-smoke.json
```

MSVC enlaza su runtime C estáticamente con target explícito y rechaza RUSTFLAGS globales durante el empaquetado. Se revisan arquitectura x64, subsistema GUI e imports directos y diferidos del PE; las DLL externas sin resolver provocan un error. GNU añade también los avisos de MinGW-w64.

`verify-portable.ps1` comprueba huellas, extrae a una carpeta temporal nueva y ejecuta con perfil aislado y PATH limitado al sistema. Conserva la extracción para inspección e imprime su ruta. Solo detiene su propio proceso. Crear el catálogo y comprobar que el proceso sigue vivo demuestra arranque en ese entorno, pero no valida todo el renderizado ni los flujos. `-InspectOnly` comprueba integridad y `--build-info` sin iniciar el recorrido gráfico.

## Linux

El target inicial es `x86_64-unknown-linux-gnu`, compilado sobre Ubuntu 22.04 para glibc 2.35 como mínimo. Instala las [dependencias de compilación y escritorio](install-linux.md), el cargador Vulkan y un controlador funcional. El tar.gz conserva permisos de ejecución e incluye `bin/rastera`, iconos, launcher y `install-user.sh`.

```sh
python3 scripts/package-unix.py --target x86_64-unknown-linux-gnu --output work/releases
python3 scripts/verify-unix-package.py --archive work/releases/rastera-0.1.0-alpha.3-linux-x64.tar.gz --expected-commit "$(git rev-parse HEAD)" --smoke --report work/releases/linux-smoke.json
```

El verificador comprueba checksum, estructura, metadatos y ejecutable. `--smoke` es exclusivo de Linux: usa un perfil temporal y un proceso controlado para comprobar el arranque con las imágenes incluidas. Necesita un DISPLAY/entorno gráfico funcional. En CI se ejecuta en Xvfb con un controlador Vulkan software; ese resultado no acredita Wayland, los diálogos del portal ni el controlador de un usuario.

Revisa que no falten dependencias del ELF, su arquitectura y las versiones de símbolos glibc. Prueba el instalador por usuario en un HOME temporal, la actualización `--replace` y el registro opcional `--register-mime`. No ejecutes la instalación de prueba en el perfil personal del desarrollador.

## macOS

Prepara por separado `aarch64-apple-darwin` en Apple Silicon y `x86_64-apple-darwin` en Intel, con las Xcode Command Line Tools. El mínimo declarado de despliegue es macOS 13. La matriz macOS 15 comprueba hosts de ambas arquitecturas y no acredita ese mínimo.

```sh
python3 scripts/package-unix.py --target aarch64-apple-darwin --output work/releases
python3 scripts/verify-unix-package.py --archive work/releases/rastera-0.1.0-alpha.3-macos-arm64.zip --expected-commit "$(git rev-parse HEAD)" --report work/releases/macos-arm64.json
```

En Intel sustituye target por `x86_64-apple-darwin` y archivo/informe por `macos-x64`. El ZIP contiene `Rastera.app` con Info.plist, ICNS y recursos. Revisa arquitectura Mach-O, dependencias del sistema, mínimo de despliegue, metadatos, `--build-info` y la integridad de la firma ad-hoc. No uses `--smoke` en macOS: la inspección no constituye una prueba gráfica.

El bundle carece de Developer ID y notarización. No declara tipos de documento ni soporta eventos de archivos de Finder; diálogo, arrastre a la ventana y argumento del ejecutable son las vías actuales. Sigue el procedimiento individual de [Apple para abrir una app descargada](https://support.apple.com/en-us/102445) después de verificar su origen; no desactives Gatekeeper ni retires cuarentena. Añadir firma Developer ID, notarización y eventos de Finder requiere un cambio separado.

## Recorrido manual pendiente

Una máquina de desarrollo o un runner con herramientas preinstaladas no equivalen a una instalación de usuario. En cada plataforma, con el paquete normal y fotografías de práctica:

1. Comprueba ZIP/tar.gz y `.sha256`; instala o extrae sin Rust ni herramientas de desarrollo. La ventana debe abrir sin consola extra, bibliotecas ausentes o errores gráficos.
2. Abre carpeta e imagen mediante diálogo, ruta y arrastre. En Linux comprueba portal FileChooser y X11/Wayland; en macOS, permisos de carpetas y Cmd. No exijas Finder Open With, que no está implementado.
3. Recorre Biblioteca y Visor, amplía, cambia el tamaño de ventana y prueba escalado/Retina. Comprueba foco, selección por rango, búsqueda y atajos, incluidos campos de texto.
4. Guarda favoritos y etiquetas; reinicia y comprueba persistencia. Confirma catálogo y caché fuera de la instalación; conserva un catálogo existente al actualizar.
5. Exporta tres fotografías a un destino vacío y revisa dimensiones/color. Repite con nombres ocupados y comprueba que los archivos existentes y originales conservan sus huellas.
6. Añade, modifica, renombra y retira copias en la carpeta abierta; comprueba actualización y recuperación si desaparece el directorio.
7. Cancela un lote, reinicia y revisa recuperación manual. Verifica que los conflictos no sobrescriben datos.
8. Prueba teclado y lector de pantalla y registra controles inaccesibles. Comprueba cierre sin trabajadores ni procesos de prueba residuales.

Registra versión/SHA-256, commit, fecha, sistema/build, arquitectura, GPU/controlador, escala y resultados. En Linux añade escritorio y X11/Wayland. Distingue el resultado de cada escenario y adjunta capturas sin datos personales. Windows limpio, macOS 13 y los recorridos de escritorios Linux/Mac siguen pendientes hasta que exista ese registro.

En Windows con Sandbox habilitado, `pwsh -File scripts/prepare-windows-sandbox.ps1 -Archive <ZIP>` prepara un `.wsb` sin ejecutarlo. Comparte una copia del ZIP en lectura y una carpeta nueva de informes en escritura; desactiva red y portapapeles. Al abrirlo, Windows PowerShell comprueba el arranque y deja `reports/verification.json`. Abre `rastera.exe` desde `extracted_to` en Sandbox para el recorrido. Preparar el archivo no demuestra que la prueba haya pasado; Sandbox no estaba disponible en el equipo de desarrollo de la primera alpha.

## GitHub y publicación

El workflow **Desktop alpha**, conservado en `.github/workflows/windows-alpha.yml`, se ejecuta con tags `v*` o manualmente. La matriz prepara cuatro paquetes y sus checksums; la CI de Rust cubre Windows, Ubuntu 22.04 y macOS 15 ARM/Intel. Revisa el resultado de cada job, los informes y los assets. Una ejecución manual ofrece artefactos y no crea una release.

Tras revisar versión, notas y pruebas, crea y sube el tag:

```sh
git tag -a v0.1.0-alpha.3 -m "Rastera 0.1.0-alpha.3"
git push origin v0.1.0-alpha.3
```

El tag prepara una **release preliminar en borrador**. No se publica automáticamente. Comprueba que los cuatro paquetes y sus `.sha256` pertenecen al mismo commit, descarga una copia desde el borrador y vuelve a verificar integridad antes de publicar desde Releases. Mantén explícitos los resultados y las validaciones pendientes si se publica una alpha experimental antes de terminar el recorrido manual. No presentes el mínimo declarado de macOS ni todas las distribuciones Linux como equipos ya probados.

El repositorio debe ser público para ofrecer código y descargas. Issues y notificación privada de vulnerabilidades deben estar habilitados cuando GitHub los ofrezca. La licencia MIT no cambia la visibilidad por sí sola.
