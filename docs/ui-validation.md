# Verificación de la interfaz de Rastera

Fecha: 10 de octubre de 2026. Implementación: commits 135394d y 5f932e9. Windows con escalado del 150%.

## Comprobaciones realizadas

- Biblioteca, visor, organización, búsqueda y los tres pasos de exportación capturados desde la aplicación real en ventanas de 1180 × 780 y 760 × 480 puntos.
- En la ventana estrecha, Propiedades pasa a una ventana auxiliar y el pie del lote queda visible mientras el contenido se desplaza. La configuración inicial empieza en Configurar; la revisión contrae las opciones y presenta nombres y dimensiones en columnas.
- Estados de bienvenida y carpeta vacía capturados con un catálogo aislado. Biblioteca abierta con 144 fotografías de prueba, sin modificar los originales.
- Lote real con tres PNG de práctica: tres JPEG de 1000 × 1000, calidad 90, publicados sin errores. Se usan destinos nuevos y un catálogo de demostración separado.
- Banco de navegación con las 144 copias de fotografías: 23 pasos con caché fría, 23 de preparación y 23 con caché caliente; todos terminaron en estado ready. Es una comprobación de cargas y navegación programada, no una prueba de todas las interacciones humanas ni una medida de FPS.
- 51 pruebas del workspace pasan con todas las características. Las dos pruebas añadidas comprueban ordenación combinada con filtros y conservación de etiquetas y dimensiones al cambiar Favorita y volver a abrir el catálogo.
- Clippy para todos los targets y características sin advertencias; formato, compilación predeterminada y compilación optimizada con capture correctos.
- Tutorial autónomo abierto en Brave a 1400 y 390 píxeles: 12 imágenes cargadas, sin desbordamiento horizontal ni errores de JavaScript; ampliación, cierre con Esc y descarga del original comprobados. El SHA-256 de la descarga coincide con la foto de práctica.

## Repetir la revisión

```powershell
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo build -p rastera-desktop --features capture --locked
.\scripts\capture-tutorial.ps1 -ExportDirectory work/ui-check/exports -CaptureDirectory work/ui-check/capturas
.\scripts\capture-tutorial.ps1 -ExportDirectory work/ui-check/exports-pequenos -CaptureDirectory work/ui-check/pequenas -ViewportSize 760x480
cargo build -p rastera-desktop --release --features capture --locked
.\scripts\measure-navigation.ps1 -InputPath C:\ruta\a\fotografias -OutputDirectory work/ui-check/navegacion -Runs 1
```

Los destinos de exportación y medición deben ser nuevos. Usa fotografías de prueba y sustituye la ruta del último comando. Los scripts terminan sus propias instancias al acabar. La característica capture está deshabilitada en la compilación normal.

Las siete capturas publicadas están en [el tutorial ilustrado](tutorial/index.html); las instrucciones de usuario están en [Usar Rastera](usage.md).

## Actualización del 11 de octubre de 2026

La entrega añade navegación y selección de la cuadrícula con teclado y organización conjunta. Las 58 pruebas del workspace incluyen eventos de egui para foco, desplazamiento a una imagen fuera de pantalla, apertura con Enter y bloqueo de navegación mientras se escribe o se desactiva la cuadrícula. Las pruebas del catálogo cubren combinación y retirada de etiquetas, preservación de metadatos, rutas Unicode, eliminación de rutas duplicadas, rollback por límite de etiquetas y por error SQL, metadatos pendientes y cambio de carpeta durante una escritura aceptada. Una receta sin cambios conserva exactamente la anotación previa.

Organizar selección se capturó antes y después de un guardado real a 1180 × 780 y 760 × 480 puntos. Se comprobó que los catálogos de vista previa permanecen sin la nueva etiqueta; los catálogos guardados contienen colección-demo y conservan las etiquetas particulares de las tres imágenes. Una imagen estaba oculta por el filtro y recibió el cambio por pertenecer a la selección. Los archivos originales permanecen iguales al contenido versionado. La validación del teclado usa eventos de egui; queda pendiente comprobar lectores de pantalla e interacción prolongada en otros entornos.

Las compilaciones normal y optimizada, el formato y Clippy con todas las características pasan. El tutorial actualizado se comprobó en Brave a 1400 y 390 píxeles: 14 imágenes cargadas, sin desbordamiento horizontal ni errores de JavaScript, ampliación y cierre con Esc correctos y descarga con SHA-256 idéntico al original.
