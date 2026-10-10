# Usar Rastera

## Navegar y seleccionar

Abre una carpeta desde el botón, la ruta, arrastrándola a la ventana o pasándola como argumento de inicio. La cuadrícula carga las miniaturas visibles; no recorre subcarpetas.

Un clic selecciona una imagen y actualiza sus detalles. Ctrl+clic añade o retira imágenes. Seleccionar todo actúa sobre las imágenes que muestra la cuadrícula, incluidos los resultados del filtro. Limpiar selección retira todas las seleccionadas.

La selección se mantiene al cambiar filtros; el contador incluye imágenes seleccionadas que hayan quedado ocultas. El plan de exportación permite revisar la selección completa.

Doble clic abre el visor. Usa Cuadrícula para volver, las flechas para navegar, la rueda para ampliar, el arrastre para desplazar y Home para ajustar. El zoom trabaja sobre una vista previa de hasta 4.096 píxeles; la exportación usa el original.

Actualizar vuelve a leer la carpeta y limpia caché y selección. Esta acción refleja cambios externos.

## Organizar

Selecciona una imagen y pulsa Organizar imagen, o Favoritos y etiquetas en la cuadrícula. Marca Favorita, escribe etiquetas separadas por comas y pulsa Guardar. Se admiten hasta 32 etiquetas de 64 caracteres; se recortan espacios y se eliminan duplicados.

La búsqueda filtra por nombre o etiqueta sin distinguir mayúsculas. También puedes filtrar por favoritos y extensión de archivo. Los filtros actúan sobre la carpeta abierta.

En Windows, la organización vive en %LOCALAPPDATA%\Rastera\catalog.sqlite, junto a los archivos auxiliares de SQLite. No se escribe en las imágenes. Las rutas identifican las anotaciones; mover o renombrar un archivo externamente crea una ruta distinta.

Si falla el catálogo, la aplicación muestra el error y permite continuar usando carpeta y visor. No se modifica una base de una versión de esquema más reciente.

## Exportar

1. Selecciona las imágenes y pulsa Herramientas por lotes. Si no hay selección múltiple, se usa la imagen del inspector.
2. Elige una carpeta de destino existente, distinta de la carpeta de los originales.
3. Configura PNG, JPEG o WebP sin pérdidas, la rotación horaria y el límite del lado mayor. El redimensionado mantiene la proporción y nunca amplía.
4. Para JPEG, elige la calidad y el fondo que reemplazará la transparencia.
5. Lee y acepta la política de color y metadatos. Pulsa Revisar plan para comprobar nombres y dimensiones.
6. Pulsa Exportar copias. Puedes seguir navegando mientras avanza el lote.

La receta normaliza la orientación EXIF antes de rotar y redimensionar. Solo admite entradas de 8 bits sin perfil ICC y asume sRGB; se rechazan declaraciones PNG de color no compatibles. Las salidas eliminan metadatos originales, incluidos EXIF, GPS y XMP. PNG y WebP conservan transparencia; JPEG se recodifica con el fondo indicado.

Las salidas se llaman nombre-rastera.ext. Las entradas que producirían nombres equivalentes o destinos ya existentes impiden aprobar el plan. Una colisión durante la ejecución produce un error sin reemplazar el archivo.

Los lotes estiman 32 bytes de buffers por píxel bajo el presupuesto compartido de 512 MiB. Una entrada grande puede rechazarse aunque el visor pueda abrirla. Se procesa un archivo a la vez. El siguiente espera mientras carga la imagen seleccionada; una llamada de códec en curso termina antes de liberar su reserva.

Cancelar comprueba el estado entre etapas y antes de publicar. Las copias publicadas permanecen y los archivos restantes se marcan cancelados. Cerrar la ventana de lotes permite continuar en segundo plano; Ver lote en curso la vuelve a abrir. Al cerrar Rastera se cancela el trabajo pendiente y se espera a los trabajadores.

## Resultados y diario

Cada lote crea rastera-export-*.sqlite en el destino, con receta, entradas, salidas y estados por archivo. La interfaz muestra la ruta del diario, progreso, salidas publicadas y errores. Un fallo individual permite continuar; un fallo del diario detiene el lote.

Las imágenes se codifican en temporales .rastera-*.tmp dentro del destino, se vacían al disco y se publican mediante persist_noclobber. Esta operación nunca reemplaza una salida. La publicación suele ser atómica en Windows y sistemas Linux actuales; no se promete persistencia ante pérdida de alimentación ni atomicidad universal en cualquier volumen.

Todavía no hay recuperación o reanudación automática. Un cierre inesperado puede dejar temporales o un ítem ready cuyo destino ya se haya publicado. El diario permite inspeccionar source_text, destination_text, state y message en items. Es necesario reconciliar archivos y diario antes de reanudar; un estado distinto de published no demuestra que la salida esté ausente.
