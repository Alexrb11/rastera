# Usar Rastera

La interfaz usa barras compactas de Archivo, Ver y Herramientas. Biblioteca sirve para seleccionar y filtrar; Visor, para examinar una imagen. Propiedades muestra dimensiones, tamaño, favorito y etiquetas en un panel lateral que puedes ocultar. En ventanas estrechas se abre como una ventana aparte.

Herramientas → Diagnóstico de rendimiento abre los contadores de memoria, cargas y caché de esta sesión. Consulta [cómo medir recursos y tiempos](performance.md) para distinguir las estimaciones internas de la RAM del proceso y repetir las mediciones.

## Navegar y seleccionar

Abre una carpeta desde Archivo, el botón, Ctrl+Shift+O, arrastrándola a la ventana o pasándola como argumento de inicio. Archivo → Abrir una ruta y la barra de ubicación permiten introducir una ruta. Ctrl+O abre una imagen. La cuadrícula carga las miniaturas visibles; no recorre subcarpetas.

Un clic selecciona una imagen y actualiza sus propiedades. Ctrl+clic añade o retira imágenes. Shift+clic selecciona un rango según el orden visible; Ctrl+Shift+clic añade ese rango a la selección. Seleccionar todo y Ctrl+A actúan sobre los resultados del filtro. Deseleccionar y Esc retiran todas las seleccionadas. El menú contextual permite abrir en el visor y copiar la ruta. Las miniaturas ofrecen tres tamaños.

La selección se mantiene al cambiar filtros; el contador incluye imágenes seleccionadas que hayan quedado ocultas y muestra cuántas están fuera del filtro. Exportar selección incluye todas ellas. Restablecer limpia búsqueda, favorito y formato; no borra la selección. Cuando no hay coincidencias se ofrece restablecer los filtros.

Doble clic abre el visor. Usa Biblioteca o Esc para volver, las flechas para navegar, la rueda para ampliar, el arrastre para desplazar y Home para ajustar. Los botones anterior y siguiente se desactivan en los extremos. Se muestra primero una vista previa de hasta 4.096 píxeles. Al ampliar por encima de su detalle, se vuelve a decodificar el original en un trabajador y se conservan sus píxeles normalizados por EXIF y color.

100% representa un píxel de imagen por píxel físico de pantalla, también con escalado de Windows. El zoom con rueda mantiene el punto bajo el cursor. Mientras se carga el detalle o si este falla, se conserva la vista previa; el inspector muestra el estado. La carga completa respeta los límites y el presupuesto compartido, por lo que algunas imágenes se rechazan por memoria. Se comprueba tamaño/fecha respecto a la vista inicial; no se detectan cambios que conserven ambos.

La GPU recibe solo mosaicos visibles, con un máximo de 60 entradas de 512 píxeles más bordes de muestreo. Si una ventana necesita más mosaicos, las regiones restantes usan la vista previa. Ajustar, Home, doble clic para ajustar, Cuadrícula, navegar o iniciar un lote liberan el detalle completo. Durante un lote se conserva la vista previa para dejar memoria al procesador.

Actualizar vuelve a leer la carpeta y limpia las texturas y la selección. Las miniaturas persistentes se invalidan por ruta, tamaño, fecha de modificación, resolución y versión del procesamiento. Esta señal detecta cambios habituales, pero no modificaciones que conserven tamaño y fecha.

La caché de miniaturas está junto al catálogo, en thumbnails-v1. Se puede borrar con la aplicación cerrada sin perder favoritos ni etiquetas. Su objetivo es 2 GiB, con limpieza cada 64 peticiones y un margen máximo de unos 16 MiB de nuevas miniaturas por trabajador entre limpiezas. Se expulsan las entradas más antiguas por fecha de escritura. Las entradas dañadas se regeneran; si la caché falla, la imagen se carga desde el original.

## Organizar

Selecciona una imagen y pulsa Propiedades → Editar organización. Marca Favorita, escribe etiquetas separadas por comas y pulsa Guardar cambios. Se admiten hasta 32 etiquetas de 64 caracteres; se recortan espacios y se eliminan duplicados. El panel muestra las etiquetas y las miniaturas identifican los favoritos.

La búsqueda filtra por nombre o etiqueta sin distinguir mayúsculas. También puedes filtrar por favoritos y extensión de archivo. Los filtros actúan sobre la carpeta abierta.

En Windows, la organización vive en %LOCALAPPDATA%\Rastera\catalog.sqlite, junto a los archivos auxiliares de SQLite. No se escribe en las imágenes. Las rutas identifican las anotaciones; mover o renombrar un archivo externamente crea una ruta distinta.

Si falla el catálogo, la aplicación muestra el error y permite continuar usando carpeta y visor. No se modifica una base de una versión de esquema más reciente.

## Exportar

1. En Biblioteca, selecciona las imágenes y pulsa Exportar selección. En Visor, Exportar imagen usa únicamente la imagen abierta, aunque quede una selección anterior en Biblioteca. Herramientas → Exportar imágenes usa la selección o, si está vacía, la imagen abierta.
2. Elige una carpeta de destino existente, distinta de la carpeta de los originales.
3. Configura PNG, JPEG o WebP sin pérdidas, la rotación horaria y el límite del lado mayor. El redimensionado mantiene la proporción y nunca amplía.
4. Para JPEG, elige la calidad y el fondo que reemplazará la transparencia.
5. Lee y acepta la política de color y metadatos. Pulsa Revisar salidas para comprobar nombres y dimensiones.
6. Pulsa Exportar N copias. Puedes seguir navegando mientras avanza el lote.

El panel distingue Configurar, Revisar y Resultados. Los botones de revisar, exportar, cancelar y cerrar permanecen fuera del área desplazable. Cada fila muestra el estado de la copia y permite consultar la ruta completa al pasar el cursor. Al terminar se cuentan las publicadas y las no publicadas, y se puede preparar otro lote con las mismas imágenes.

La receta normaliza la orientación EXIF antes de rotar y redimensionar. Visor, miniaturas y lotes admiten SDR de 8 bits y convierten perfiles ICC RGB compatibles de matriz y curvas a sRGB antes de reducir la imagen. Sin ICC se asume sRGB. Las salidas eliminan metadatos originales, incluidos EXIF, GPS y XMP, y reciben un perfil sRGB nuevo. PNG y WebP conservan transparencia; JPEG se recodifica con el fondo indicado.

La conversión usa intención colorimétrica relativa y limita los colores al rango sRGB; algunos colores de espacios amplios quedan recortados. Se rechazan perfiles inválidos, LUT, CMYK, HDR, grises con ICC y declaraciones PNG no compatibles, incluidas cICP y gamma/primarias no sRGB sin ICC. El inspector confirma la conversión del ICC cuando la vista se carga. No hay conversión al perfil del monitor ni soporte de HDR; no se promete fidelidad profesional.

Las salidas se llaman nombre-rastera.ext. Las entradas que producirían nombres equivalentes o destinos ya existentes impiden aprobar el plan. Una colisión durante la ejecución produce un error sin reemplazar el archivo.

Los lotes estiman 32 bytes de buffers por píxel bajo el presupuesto compartido de 512 MiB. Una entrada grande puede rechazarse aunque el visor pueda abrirla. Se procesa un archivo a la vez. El siguiente espera mientras carga la imagen seleccionada; una llamada de códec en curso termina antes de liberar su reserva.

Cancelar lote comprueba el estado entre etapas y antes de publicar. Las copias publicadas permanecen y los archivos restantes se marcan cancelados. Ocultar panel permite continuar en segundo plano; Herramientas → Recuperar / ver lote lo vuelve a abrir. Al cerrar Rastera se cancela el trabajo pendiente y se espera a los trabajadores.

## Resultados y diario

Cada lote crea rastera-export-*.sqlite en el destino, con receta, entradas, salidas y estados por archivo. La interfaz muestra la ruta del diario, progreso, salidas publicadas y errores. Un fallo individual permite continuar; un fallo del diario detiene el lote.

Las imágenes se codifican en temporales .rastera-*.tmp dentro del destino, se vacían al disco y se publican mediante persist_noclobber. Esta operación nunca reemplaza una salida. La publicación suele ser atómica en Windows y sistemas Linux actuales; no se promete persistencia ante pérdida de alimentación ni atomicidad universal en cualquier volumen.

Tras una interrupción, abre Herramientas → Recuperar / ver lote → Recuperar un lote y elige el diario SQLite del destino. Rastera comprueba huellas BLAKE3 de las salidas publicadas y de los originales pendientes, y presenta el resultado. Una entrada ready cuya copia ya exista y coincida con su huella se reconoce como publicada. Las salidas ajenas, originales cambiados y entradas no comprobables aparecen como conflictos.

Aceptar la política y pulsar Exportar N copias procesa únicamente los pendientes comprobados, con la receta guardada, creando un nuevo diario. Las copias verificadas no se vuelven a escribir y los conflictos se conservan. No se borran temporales ni se modifica el diario anterior. Un diario activo en otra instancia no puede recuperarse. Solo son recuperables los diarios de versión 2 creados desde esta entrega y en el mismo sistema operativo; los anteriores carecen de huellas fiables. La aplicación no busca diarios al iniciarse ni reanuda una exportación sin revisión.
