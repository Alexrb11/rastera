# Tutorial ilustrado de Rastera

Abre index.html en un navegador conservando las carpetas capturas, ejemplos y copias a su lado. La guía explica selección y visor, organización y búsqueda, exportación y eliminación de metadatos. Incluye nueve capturas reales y tres fotografías de práctica.

La página usa HTML, CSS y JavaScript locales, sin servicios externos. Las capturas se pueden ampliar; las fotos y la copia de ejemplo se pueden descargar desde la página. El botón de imprimir abre la impresión del navegador para papel o PDF.

## Procedencia

Las fotografías de taza, cuenco y jarrón se generaron mediante la herramienta integrada image_gen, en modo built-in. Los archivos finales y el conjunto completo de prompts están registrados en [procedencia.json](procedencia.json). Son ejemplos generados con IA, no fotos aportadas por el usuario.

Las capturas proceden del renderizado de Rastera 0.1.0 en Windows al 150%, con una ventana de 1180 × 780 puntos. Se actualizaron el 11 de octubre de 2026. No son maquetas de la interfaz. El catálogo de demostración se preparó aparte para mostrar favoritos y etiquetas; no modifica el catálogo del usuario. Las copias JPEG se exportaron realmente con Rastera: calidad 90, límite de 1000 píxeles, sin rotación y fondo blanco. Los PNG originales permanecen iguales a los archivos generados.

La taza original ocupa 1.763.440 bytes; la copia, 110.990 bytes. La reducción es aproximadamente del 94%, específica de este ejemplo.

## Repetir las capturas

Desde la raíz del repositorio, con Rust y Python disponibles:

    cargo build -p rastera-desktop --features capture --locked
    .\scripts\capture-tutorial.ps1 -ExportDirectory work/tutorial/Para-web-nuevo

El destino debe ser nuevo; el script no borra ni reemplaza exportaciones anteriores. Las capturas se regeneran en docs/tutorial/capturas. Para actualizar las copias de referencia, copia después los tres JPEG del destino a docs/tutorial/copias.

El script usa un catálogo aislado en work/tutorial, restaura las variables de entorno y solo termina procesos que él mismo ha iniciado si agotan el plazo de captura. Requiere Windows. -Python permite indicar el ejecutable de Python; -Executable permite elegir el binario, relativo a la raíz del repositorio. -CaptureDirectory cambia la carpeta de capturas y -ViewportSize permite verificar otros tamaños, por ejemplo 760x480.

La característica de desarrollo capture está deshabilitada por defecto. RASTERA_CAPTURE_PANEL=organization abre el editor. RASTERA_CAPTURE_TUTORIAL=configure muestra la receta antes de preparar el plan; review lo prepara y muestra la revisión; results ejecuta el plan y exporta las copias al destino explícito. review y results desplazan el panel hasta la revisión o el resultado. Estas opciones son para generar documentación y solo deben usarse con carpetas de prueba.

## Generar la entrega

    python scripts/bundle-tutorial.py C:\ruta\de\salida

Produce un HTML con todas las imágenes incrustadas y un ZIP con ese HTML, originales, copias de referencia, una carpeta de destino vacía e instrucciones. Ambos funcionan sin conexión.

## Organización conjunta

Las dos capturas adicionales muestran Organizar selección antes y después de un guardado real. El catálogo aislado tiene tres imágenes seleccionadas y el filtro aprobada deja una oculta. La receta marca todas como favoritas y añade colección-demo, conservando las etiquetas previas. El resultado se comprobó leyendo los tres registros de SQLite.

    .\scripts\capture-organization.ps1 -CaptureDirectory docs/tutorial/capturas
    .\scripts\capture-organization.ps1 -CaptureDirectory work/organization/pequenas -ViewportSize 760x480

El script crea un catálogo nuevo por escena, restaura el entorno y conserva los originales. RASTERA_CAPTURE_PANEL=bulk-organization abre esta demostración; RASTERA_CAPTURE_BULK=apply ejecuta el guardado. Solo se inicia la demostración si también hay un destino RASTERA_CAPTURE_TO. Estas opciones requieren capture y deben utilizarse únicamente con catálogos de prueba.
