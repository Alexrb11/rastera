# Usar Rastera

La interfaz separa los menús Archivo, Ver, Herramientas y Ayuda, la ubicación y las herramientas de Biblioteca. Biblioteca sirve para seleccionar, filtrar y ordenar; Visor, para examinar una imagen. Propiedades muestra dimensiones, tamaño, favorito y etiquetas en un panel lateral que puedes ocultar o ajustar arrastrando su borde. En ventanas de menos de 980 puntos se abre como una ventana aparte. Ayuda → Atajos de teclado, o F1, muestra las combinaciones disponibles.

Herramientas → Diagnóstico de rendimiento abre los contadores de memoria, cargas y caché de esta sesión. Consulta [cómo medir recursos y tiempos](performance.md) para distinguir las estimaciones internas de la RAM del proceso y repetir las mediciones.

## Navegar y seleccionar

Abre una carpeta desde Archivo, el botón, Ctrl+Shift+O, arrastrándola a la ventana o pasándola como argumento de inicio. Archivo → Abrir una ruta, F4 y la barra de ubicación permiten introducir una ruta; se aceptan rutas pegadas entre comillas. Ctrl+O abre una imagen. Ver carpeta permite explorar la carpeta de un archivo abierto directamente. La flecha hacia arriba abre la carpeta superior. La cuadrícula carga las miniaturas visibles; no recorre subcarpetas.

Un clic selecciona una imagen y actualiza sus propiedades. Ctrl+clic añade o retira imágenes. Shift+clic selecciona un rango según el orden visible; Ctrl+Shift+clic añade ese rango a la selección. Selección → Seleccionar todo y Ctrl+A actúan sobre los resultados del filtro. Selección → Deseleccionar y Esc retiran todas las seleccionadas. El menú contextual permite abrir en el visor, copiar la ruta, organizar una imagen y exportar la selección. Al abrirlo sobre una imagen no seleccionada, esta pasa a ser la selección. Las miniaturas ofrecen tres tamaños, nombre, formato y dimensiones orientadas, además de una estrella para los favoritos. Nombre A–Z / Nombre Z–A cambia el orden visible.

La selección se mantiene al cambiar filtros; el contador incluye imágenes seleccionadas que hayan quedado ocultas y muestra cuántas están fuera del filtro. Exportar selección incluye todas ellas. Restablecer limpia búsqueda, favorito y formato; no borra la selección. Cuando no hay coincidencias se ofrece restablecer los filtros.

Doble clic abre el visor. Usa Biblioteca o Esc para volver, las flechas para navegar, la rueda para ampliar, el arrastre para desplazar y Home para ajustar. Al entrar desde Biblioteca, la navegación sigue el filtro y el orden visible. Los botones anterior y siguiente se desactivan en los extremos. Se muestra primero una vista previa de hasta 4.096 píxeles. Al ampliar por encima de su detalle, se vuelve a decodificar el original en un trabajador y se conservan sus píxeles normalizados por EXIF y color.

100% representa un píxel de imagen por píxel físico de pantalla, también con escalado de Windows. El zoom con rueda mantiene el punto bajo el cursor. Mientras se carga el detalle o si este falla, se conserva la vista previa; el inspector muestra el estado. La carga completa respeta los límites y el presupuesto compartido, por lo que algunas imágenes se rechazan por memoria. Se comprueba tamaño/fecha respecto a la vista inicial. Cuando se navega desde una carpeta, el observador también provoca una recarga si recibe una notificación de cambio.

La GPU recibe solo mosaicos visibles, con un máximo de 60 entradas de 512 píxeles más bordes de muestreo. Si una ventana necesita más mosaicos, las regiones restantes usan la vista previa. Ajustar, Home, doble clic para ajustar, Cuadrícula, navegar o iniciar un lote liberan el detalle completo. Durante un lote se conserva la vista previa para dejar memoria al procesador.

La carpeta abierta se actualiza automáticamente cuando otra aplicación añade, modifica, renombra o retira imágenes. Los cambios rápidos se agrupan antes de leer el disco. Se conservan búsqueda, filtros, orden, selección de archivos que siguen presentes y miniaturas sin cambios. La imagen abierta se vuelve a cargar si cambia, conservando zoom y desplazamiento; el detalle anterior se descarta. Si desaparece, se abre la siguiente imagen superviviente del orden de navegación, o la anterior si no queda ninguna después. Una carpeta vacía puede recibir nuevas imágenes sin volver a abrirla.

La barra inferior indica Actualización automática. Si no se pueden recibir notificaciones, muestra Revisión cada 15 s; la revisión periódica también funciona como respaldo cuando hay observador. Si la carpeta desaparece, pierde permisos o supera los límites, se conserva la última biblioteca utilizable, se muestra el error y se reintenta. Una enumeración parcial no se utiliza para borrar entradas de esa biblioteca.

Se observa únicamente la carpeta abierta, sin recorrer subcarpetas. Abrir un archivo directamente no activa el observador: pulsa Ver carpeta para usar esta función. Un renombrado externo se trata como retirada y nueva ruta; sus favoritos y etiquetas no se trasladan. Las anotaciones de rutas retiradas siguen en SQLite y reaparecen si se vuelve a usar esa ruta. Los paneles de organización conjunta y exportación mantienen la copia de rutas elegida al abrirlos.

Actualizar, o F5, sigue siendo una lectura manual que limpia texturas y selección. Las miniaturas persistentes se identifican por ruta, tamaño, fecha, resolución y versión del procesamiento. Las notificaciones de la sesión invalidan además las entradas afectadas, incluso si conservan tamaño y fecha. La revisión de respaldo compara tamaño y fecha, sin calcular huellas de todo el contenido: un cambio que conserve ambos y cuya notificación se pierda puede pasar inadvertido.

La caché de miniaturas está junto al catálogo, en thumbnails-v1. Se puede borrar con la aplicación cerrada sin perder favoritos ni etiquetas. Su objetivo es 2 GiB. Se revisa al comenzar, después de escribir 16 MiB de registros y al atender una petición si han pasado 60 segundos desde la última revisión. Leer miniaturas guardadas no provoca recorridos por número de peticiones. El margen de escrituras entre revisiones es 16 MiB más un registro como máximo por trabajador; varias instancias no comparten un límite estricto. Solo se ordenan y expulsan las entradas antiguas cuando se supera la cuota. Las entradas dañadas se regeneran; si la caché falla, la imagen se carga desde el original. La revisión comprueba la cancelación mientras enumera y elimina.

En Biblioteca, las flechas recorren las miniaturas según las columnas visibles. Home y End van al primer y último resultado; RePág y AvPág avanzan por páginas y desplazan la cuadrícula para mantener visible la imagen enfocada. Shift extiende un rango; Ctrl+flechas mueve el foco sin cambiar la selección y Espacio añade o retira la imagen enfocada. Enter abre esa imagen en Visor. Puedes llegar a la cuadrícula con Tab o pulsando una miniatura. Los atajos de navegación quedan suspendidos al escribir en búsqueda y al abrir un panel auxiliar.

## Organizar

Selecciona una imagen y pulsa Propiedades → Editar organización. Puedes marcar Favorita directamente en Propiedades. Marca Favorita, escribe etiquetas separadas por comas y pulsa Guardar cambios. Se admiten hasta 32 etiquetas de 64 caracteres; se recortan espacios y se eliminan duplicados. El panel muestra las etiquetas y las miniaturas identifican los favoritos.

Para varias imágenes, selecciona el grupo y pulsa Organizar…, Herramientas → Organizar selección o Ctrl+Shift+T en Biblioteca. El panel toma una copia de esa selección, incluyendo las imágenes ocultas por filtros. Puedes dejar Favoritas sin cambios, marcar todas o retirarlas, y añadir o retirar las etiquetas indicadas sin reemplazar las demás. Retirar compara el texto exacto de las etiquetas, incluidas mayúsculas. La vista previa cuenta las imágenes que cambiarían y permite revisar las rutas incluidas.

Guardar organización lee los datos actuales y aplica toda la selección en una transacción de SQLite, fuera del hilo de interfaz. Si una imagen superaría 32 etiquetas, o SQLite falla, no se guarda ninguna de las modificaciones de esa operación. Los archivos originales se conservan. Durante el guardado puedes ocultar el panel o cambiar de carpeta; la escritura aceptada termina y el estado se puede volver a abrir desde la barra inferior. La edición individual espera mientras se guarda el grupo. Los metadatos pendientes se guardan después de recibir las nuevas anotaciones, conservando sus etiquetas.

Ctrl+F enfoca la búsqueda, que filtra por nombre o etiqueta sin distinguir mayúsculas. También puedes filtrar por favoritos y extensión de archivo. Los filtros actúan sobre la carpeta abierta.

En Windows, la organización vive en %LOCALAPPDATA%\Rastera\catalog.sqlite, junto a los archivos auxiliares de SQLite. No se escribe en las imágenes. Las rutas identifican las anotaciones; mover o renombrar un archivo externamente crea una ruta distinta.

Si falla el catálogo, la aplicación muestra el error y permite continuar usando carpeta y visor. No se modifica una base de una versión de esquema más reciente.

## Exportar

1. En Biblioteca, selecciona las imágenes y pulsa Exportar (N)… o Ctrl+E. En Visor, Exportar imagen usa únicamente la imagen abierta, aunque quede una selección anterior en Biblioteca. Ctrl+E y Herramientas → Exportar imágenes usan la selección en Biblioteca, o la imagen abierta en Visor; si la selección está vacía, usan la imagen abierta.
2. Elige una carpeta de destino existente, distinta de la carpeta de los originales.
3. Configura PNG, JPEG o WebP sin pérdidas, la rotación horaria y el límite del lado mayor. El redimensionado mantiene la proporción y nunca amplía.
4. Para JPEG, elige la calidad y el fondo que reemplazará la transparencia.
5. Lee y acepta la política de color y metadatos. Pulsa Revisar salidas para comprobar nombres y dimensiones.
6. Pulsa Exportar N copias. Puedes seguir navegando mientras avanza el lote.

El panel distingue Configurar, Revisar y Resultados. Las opciones se despliegan durante la configuración y se contraen al revisar; los resultados conservan la receta usada. Esc oculta el panel sin cancelar el lote. Los botones de revisar, exportar, cancelar y cerrar permanecen fuera del área desplazable. Cada fila muestra el estado de la copia y permite consultar la ruta completa al pasar el cursor. Al terminar se cuentan las publicadas y las no publicadas, y se puede preparar otro lote con las mismas imágenes.

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
