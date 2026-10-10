# Usar Rastera

## Navegar y seleccionar

Abre una carpeta desde el botón, la ruta, arrastrándola a la ventana o pasándola como argumento de inicio. La cuadrícula carga las miniaturas visibles; no recorre subcarpetas.

Un clic selecciona una imagen y actualiza sus detalles. Ctrl+clic añade o retira imágenes. Seleccionar todo actúa sobre las imágenes que muestra la cuadrícula, incluidos los resultados del filtro. Limpiar selección retira todas las seleccionadas.

La selección se mantiene al cambiar filtros; el contador incluye imágenes seleccionadas que hayan quedado ocultas. El plan de exportación permite revisar la selección completa.

Doble clic abre el visor. Usa Cuadrícula para volver, las flechas para navegar, la rueda para ampliar, el arrastre para desplazar y Home para ajustar. El zoom trabaja sobre una vista previa de hasta 4.096 píxeles; la exportación usa el original.

Actualizar vuelve a leer la carpeta y limpia las texturas y la selección. Las miniaturas persistentes se invalidan por ruta, tamaño, fecha de modificación, resolución y versión del procesamiento. Esta señal detecta cambios habituales, pero no modificaciones que conserven tamaño y fecha.

La caché de miniaturas está junto al catálogo, en thumbnails-v1. Se puede borrar con la aplicación cerrada sin perder favoritos ni etiquetas. Su objetivo es 2 GiB, con limpieza cada 64 peticiones y un margen máximo de unos 16 MiB de nuevas miniaturas por trabajador entre limpiezas. Se expulsan las entradas más antiguas por fecha de escritura. Las entradas dañadas se regeneran; si la caché falla, la imagen se carga desde el original.

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

La receta normaliza la orientación EXIF antes de rotar y redimensionar. Visor, miniaturas y lotes admiten SDR de 8 bits y convierten perfiles ICC RGB compatibles de matriz y curvas a sRGB antes de reducir la imagen. Sin ICC se asume sRGB. Las salidas eliminan metadatos originales, incluidos EXIF, GPS y XMP, y reciben un perfil sRGB nuevo. PNG y WebP conservan transparencia; JPEG se recodifica con el fondo indicado.

La conversión usa intención colorimétrica relativa y limita los colores al rango sRGB; algunos colores de espacios amplios quedan recortados. Se rechazan perfiles inválidos, LUT, CMYK, HDR, grises con ICC y declaraciones PNG no compatibles, incluidas cICP y gamma/primarias no sRGB sin ICC. El inspector confirma la conversión del ICC cuando la vista se carga. No hay conversión al perfil del monitor ni soporte de HDR; no se promete fidelidad profesional.

Las salidas se llaman nombre-rastera.ext. Las entradas que producirían nombres equivalentes o destinos ya existentes impiden aprobar el plan. Una colisión durante la ejecución produce un error sin reemplazar el archivo.

Los lotes estiman 32 bytes de buffers por píxel bajo el presupuesto compartido de 512 MiB. Una entrada grande puede rechazarse aunque el visor pueda abrirla. Se procesa un archivo a la vez. El siguiente espera mientras carga la imagen seleccionada; una llamada de códec en curso termina antes de liberar su reserva.

Cancelar comprueba el estado entre etapas y antes de publicar. Las copias publicadas permanecen y los archivos restantes se marcan cancelados. Cerrar la ventana de lotes permite continuar en segundo plano; Ver lote en curso la vuelve a abrir. Al cerrar Rastera se cancela el trabajo pendiente y se espera a los trabajadores.

## Resultados y diario

Cada lote crea rastera-export-*.sqlite en el destino, con receta, entradas, salidas y estados por archivo. La interfaz muestra la ruta del diario, progreso, salidas publicadas y errores. Un fallo individual permite continuar; un fallo del diario detiene el lote.

Las imágenes se codifican en temporales .rastera-*.tmp dentro del destino, se vacían al disco y se publican mediante persist_noclobber. Esta operación nunca reemplaza una salida. La publicación suele ser atómica en Windows y sistemas Linux actuales; no se promete persistencia ante pérdida de alimentación ni atomicidad universal en cualquier volumen.

Tras una interrupción, abre Herramientas por lotes → Recuperar lote y elige el diario SQLite del destino. Rastera comprueba huellas BLAKE3 de las salidas publicadas y de los originales pendientes, y presenta el resultado. Una entrada ready cuya copia ya exista y coincida con su huella se reconoce como publicada. Las salidas ajenas, originales cambiados y entradas no comprobables aparecen como conflictos.

Aceptar la política y pulsar Exportar copias procesa únicamente los pendientes comprobados, con la receta guardada, creando un nuevo diario. Las copias verificadas no se vuelven a escribir y los conflictos se conservan. No se borran temporales ni se modifica el diario anterior. Un diario activo en otra instancia no puede recuperarse. Solo son recuperables los diarios de versión 2 creados desde esta entrega y en el mismo sistema operativo; los anteriores carecen de huellas fiables. La aplicación no busca diarios al iniciarse ni reanuda una exportación sin revisión.
