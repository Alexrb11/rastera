# Rastera · Primera alpha para Windows

Rastera es un visor y organizador local de imágenes con exportación por lotes. Esta versión es preliminar y busca feedback. El paquete se dirige a Windows 10/11 x64; la comprobación local se ha realizado en Windows 11. Linux, macOS y Windows ARM no tienen todavía un paquete validado.

## Abrir la aplicación

1. Descarga el ZIP de Windows de la [release oficial](https://github.com/Alexrb11/rastera/releases).
2. Extrae toda la carpeta a una ubicación propia, por ejemplo Documentos\Rastera. Evita ejecutar desde dentro del ZIP o en una carpeta de Windows protegida.
3. Abre `rastera.exe`. No necesitas Rust ni un instalador.
4. Pulsa Abrir carpeta para navegar o Abrir imagen para examinar un archivo. También puedes arrastrarlos a la ventana.

Abre `docs/tutorial/index.html` en un navegador para seguir la guía ilustrada sin conexión. Las imágenes de `docs/tutorial/ejemplos` permiten practicar sin usar tus fotografías. Consulta `docs/usage.md` para conocer los atajos y los límites.

El ejecutable de la alpha no está firmado digitalmente. Windows puede mostrar «editor desconocido». Comprueba el origen de la descarga; no desactives las protecciones del sistema para usar Rastera.

## Primer recorrido

Selecciona imágenes con Ctrl+clic o Shift+clic. Usa Favorita y Organizar para añadir etiquetas; la búsqueda filtra la carpeta abierta. Doble clic abre el visor. Los cambios externos en esa carpeta actualizan la biblioteca automáticamente.

Para crear copias, selecciona las imágenes, pulsa Exportar, elige otra carpeta y revisa nombres y dimensiones antes de ejecutar. Rastera procesa los originales y conserva los archivos existentes en el destino. No ofrece todavía borrado ni renombrado de originales.

## Datos y actualizaciones

El ejecutable es portable; el catálogo pertenece al perfil de Windows. Se guarda en `%LOCALAPPDATA%\Rastera\catalog.sqlite`, y las miniaturas en `%LOCALAPPDATA%\Rastera\thumbnails-v1`. Copiar solo el ejecutable a otro equipo no traslada favoritos y etiquetas.

Para actualizar, cierra Rastera y extrae el nuevo ZIP en otra carpeta. Guarda una copia del directorio de datos antes de probar una nueva alpha. Para retirar la aplicación, cierra Rastera y elimina la carpeta del ejecutable; el catálogo y los originales permanecen.

Los diarios `rastera-export-*.sqlite` se guardan con las exportaciones. Conserva el diario y las copias si necesitas revisar o recuperar un lote interrumpido.

## Límites de esta versión

- JPEG, PNG y WebP estáticos, SDR de 8 bits. No admite RAW, HEIC, GIF, animaciones, HDR ni CMYK.
- Una carpeta sin recursión, hasta 10.000 imágenes y 16 MiB de rutas; se excluyen enlaces.
- Hasta 128 MiB por archivo y 64 millones de píxeles. Una carga puede rechazarse por presupuesto de memoria.
- Favoritos y etiquetas se vinculan a la ruta. Un movimiento o renombrado externo no los traslada.
- La revisión periódica usa tamaño/fecha; puede omitir un cambio que conserve ambos si se pierde su notificación.
- Convierte perfiles ICC RGB compatibles a sRGB; la conversión al perfil del monitor sigue pendiente.
- La recuperación de lotes se inicia desde el menú y requiere revisión; no se reanuda automáticamente al abrir.
- La compatibilidad con lectores de pantalla y una instalación limpia de Windows siguen en validación.

## Informar de problemas

Usa [las incidencias del repositorio](https://github.com/Alexrb11/rastera/issues). Indica versión, Windows, GPU, pasos y resultado observado. `BUILD-INFO.json` identifica el commit y la suma del ejecutable. Oculta rutas personales y comparte solo imágenes que puedas publicar. Para vulnerabilidades, consulta `SECURITY.md`.

El ZIP tiene un archivo `.sha256` separado. Puedes obtener su suma con `Get-FileHash .\rastera-*.zip -Algorithm SHA256` y compararla con ese archivo. La suma comprueba integridad, no identidad del editor.

Rastera se distribuye bajo MIT. Las dependencias y fuentes conservan sus condiciones en `THIRD-PARTY-NOTICES.txt` y `dependency-licenses.json`.

Los avisos de la biblioteca estándar están en RUST-STANDARD-LIBRARY-NOTICES.html; GNU incluye además MINGW-RUNTIME-NOTICES.txt. La distribución MSVC enlaza su runtime C estáticamente.
