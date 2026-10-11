# Preparar y verificar una alpha

## Compilar y empaquetar

Desde un checkout limpio, con Rust 1.95 o posterior, Python 3.11 o posterior, Git y el componente rust-docs:

```powershell
pwsh -File scripts/package-windows.ps1 -Target x86_64-pc-windows-msvc
pwsh -File scripts/verify-portable.ps1 -Archive work/releases/rastera-0.1.0-alpha.2-windows-x64-msvc.zip -ReportPath work/releases/smoke.json
```

GNU también está admitido mediante `-Target x86_64-pc-windows-gnu`. MSVC enlaza su runtime C estáticamente con target explícito y rechaza RUSTFLAGS globales durante el empaquetado. MSVC necesita las herramientas de C++ y el SDK de Windows; GNU, MinGW. No se empaqueta una build con `capture`. El empaquetador verifica la versión, el formato PE x64 y las licencias del grafo de dependencias. Conserva el commit, compilador y huella del ejecutable; falla si ya existe el ZIP. Usa otra carpeta de salida para repetir una prueba.

`verify-portable.ps1` comprueba las huellas, extrae a una carpeta temporal nueva y ejecuta la aplicación con un perfil aislado y PATH limitado al sistema. Conserva la extracción para inspección; imprime su ruta. Solo detiene el proceso que ha iniciado. La creación del catálogo y un proceso vivo prueban el arranque, pero no garantizan el renderizado ni la corrección de todos los flujos.

## Windows limpio: comprobación pendiente antes de recomendarla ampliamente

La prueba local y un runner de GitHub con herramientas preinstaladas no equivalen a un equipo de usuario limpio. En una VM o un equipo Windows x64 sin Rust ni MinGW:

1. Copia el ZIP y `.sha256`; comprueba la huella con `Get-FileHash` y extrae el ZIP.
2. Abre `rastera.exe` desde el Explorador. Debe aparecer una ventana sin consola y sin avisos de DLL ausentes.
3. Abre las fotografías de `docs/tutorial`, recorre visor y cuadrícula, amplía y cambia el tamaño de la ventana al 100% y 150% de escala.
4. Marca favoritos y etiquetas; cierra y vuelve a abrir para comprobar persistencia.
5. Exporta tres fotografías a una carpeta vacía, revisa dimensiones y abre los resultados. Repite con nombres ocupados y comprueba que los existentes permanecen intactos.
6. En la carpeta abierta añade una copia, modifícala, renómbrala y retírala; comprueba que la biblioteca se actualiza.
7. Cancela un lote, reinicia y revisa la recuperación manual. Compara las huellas de originales antes y después.
8. Prueba teclado y lector de pantalla; registra los controles inaccesibles.

Registra edición y build de Windows, GPU/controlador, escala, versión y SHA-256 del ZIP, fecha, resultado y capturas sin datos personales. La falta de firma debe estar declarada; no se deben desactivar protecciones del sistema para ejecutar la prueba.

Si tienes Windows Sandbox habilitado, `pwsh -File scripts/prepare-windows-sandbox.ps1 -Archive <ZIP>` prepara un `.wsb` sin ejecutarlo. Comparte solo una copia del ZIP en lectura y una carpeta nueva para informes en escritura, con red y portapapeles desactivados. Al abrir el `.wsb`, Windows PowerShell ejecuta la comprobación de arranque y deja el informe en `reports/verification.json`. Después abre `rastera.exe` desde `extracted_to` dentro del Sandbox y realiza el recorrido manual. No necesita Python. Preparar el archivo no demuestra que la prueba haya pasado; Sandbox no estaba disponible en el equipo de desarrollo de esta alpha.

## GitHub

El workflow **Windows alpha** se ejecuta al publicar un tag `v*` o manualmente desde Actions. Primero comprueba formato, Clippy y tests; compila sin `capture`, empaqueta y verifica integridad y `--build-info` con PATH aislado. Usa `-InspectOnly`: el arranque gráfico se comprueba localmente o en la VM. La ejecución manual prepara artefactos y no crea una release.

Para publicar, comprueba que la versión en Cargo.toml coincide con el tag, revisa `docs/releases/<version>.md`, el resultado de CI y las pruebas anteriores. Después crea y sube el tag:

```powershell
git tag -a v0.1.0-alpha.2 -m "Rastera 0.1.0-alpha.2"
git push origin v0.1.0-alpha.2
```

El tag crea una **release preliminar en borrador**, con ZIP y SHA-256. Revisa sus archivos y resultados antes de publicarla desde Releases. El borrador permite preparar la distribución aunque la prueba de Windows limpio siga pendiente; el texto de la alpha debe reflejar esa limitación si se publica antes.

El repositorio debe ser público para ofrecer código y descargas a todos. Habilita Issues y la notificación privada de vulnerabilidades en la configuración de GitHub cuando estén disponibles. La licencia MIT no cambia por sí sola la visibilidad del repositorio.
