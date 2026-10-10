# Plan de desarrollo de Rastera

Este plan lleva la arquitectura desde el prototipo hasta una primera aplicación útil en Windows. Las fases se completan mediante criterios observables, no por una fecha estimada.

El avance inicial incluye workspace, CI, visor nativo, navegación progresiva por carpetas, cuadrícula virtualizada y selección múltiple. Las miniaturas tienen una caché LRU acotada y un trabajador cancelable que comparte memoria con el visor. Se ha comprobado el renderizado en Windows. Las fases siguen abiertas: faltan validación de accesibilidad, carga de resolución completa en el visor, mediciones con bibliotecas grandes, observación de cambios externos y lotes. El catálogo local ya guarda favoritos, etiquetas y dimensiones, con búsqueda y filtros en la carpeta abierta.

## Fase 1 Base técnica y visor

Crear el workspace, la ventana nativa, la separación de componentes y el primer flujo de carga en trabajadores. Incorporar JPEG, PNG y WebP estáticos, ajuste a ventana, zoom, desplazamiento y teclado.

Criterios de aceptación:

- La UI sigue respondiendo mientras se carga una imagen grande.
- Cambiar rápidamente de archivo no muestra resultados de una selección anterior.
- Los archivos corruptos, animados, no compatibles y excesivos producen errores comprensibles.
- Orientación, transparencia y límites de textura tienen casos de prueba.
- La ventana en reposo no provoca repintados continuos.
- Se valida navegación por teclado, foco y accesibilidad básica.
- Quedan fijadas versiones compatibles, versión mínima de Rust y lockfile.

## Fase 2 Carpetas y miniaturas

Añadir enumeración progresiva, cuadrícula virtualizada, selección, ordenación, cachés acotadas y prioridades. Las miniaturas visibles tienen preferencia sobre el trabajo fuera de pantalla.

Criterios de aceptación:

- La cuadrícula muestra resultados antes de terminar de escanear una carpeta.
- Se puede navegar por corpus de 100, 1.000 y 10.000 imágenes sin decodificarlas todas a la vez.
- Colas, píxeles pendientes y cachés respetan sus presupuestos instrumentados.
- Cambiar de carpeta cancela peticiones prescindibles sin invalidar la nueva vista.
- Se miden tiempo de primera cuadrícula, memoria y navegación con caché fría y caliente.

## Fase 3 Catálogo y organización

Incorporar SQLite, migraciones, favoritos, etiquetas, búsqueda, observación de carpetas y reconciliación. El catálogo guarda organización y metadatos; los originales permanecen en sus carpetas.

Criterios de aceptación:

- Etiquetas y favoritos sobreviven al reinicio y a la limpieza de cachés.
- Las consultas se paginan y no bloquean el hilo de interfaz.
- Añadir, modificar o retirar imágenes actualiza la biblioteca, también tras perder eventos del observador.
- Rutas con Unicode, permisos insuficientes y archivos ausentes tienen comportamiento definido.
- Un fallo del catálogo permite abrir una carpeta en modo básico y ofrece recuperación.

## Fase 4 Exportación por lotes

Implementar recetas de redimensionado, rotación y conversión. Añadir previsualización del plan, progreso por ítem, cancelación y diario de recuperación. Las salidas se escriben en una carpeta distinta y sin sobrescribir originales.

Criterios de aceptación:

- El visor conserva prioridad y capacidad de respuesta durante un lote.
- Las salidas coinciden con dimensiones, orientación, calidad y fondo seleccionados.
- Un error individual queda registrado y permite continuar con los demás archivos.
- Los cambios de entrada y las colisiones de destino se detectan sin sobrescritura.
- Cancelar conserva las salidas ya publicadas y refleja correctamente el estado de los ítems activos.
- Reiniciar después de interrumpir una escritura permite reconciliar temporales y destinos.
- La política de color y metadatos se cumple; una conversión no soportada se rechaza explícitamente.

Esta fase completa el MVP de visor, organizador y procesamiento por lotes.

## Fase 5 Operaciones sobre originales

Incorporar renombrado por lotes con previsualización, resolución de ciclos, diario y deshacer condicionado a la identidad del archivo. El borrado requiere un diseño separado.

Criterios de aceptación:

- Los nombres duplicados, reservados y equivalentes según el volumen se detectan antes de ejecutar.
- Cambios solo de mayúsculas y ciclos de nombres tienen cobertura en Windows.
- Un fallo intermedio permite recuperar un estado coherente sin asumir atomicidad del lote.
- Deshacer no reemplaza archivos creados o modificados por otra aplicación.

## Fase 6 Distribución y ampliaciones

Preparar paquetes de Windows, documentación de uso y controles de dependencias. Medir mejoras antes de añadir optimizaciones más complejas. Después se podrán añadir una CLI, otras plataformas y nuevos formatos.

Criterios de aceptación:

- Una build de release funciona en una instalación limpia de Windows dentro de la matriz soportada.
- Configuración, catálogo y cachés usan directorios del sistema y sobreviven a una actualización compatible.
- CI ejecuta formato, Clippy y pruebas; las pruebas gráficas y de rendimiento documentan su entorno.
- Se define la licencia del proyecto y se comprueban las condiciones de sus dependencias antes de distribuir.
- Cada plataforma adicional valida diálogos, rutas, observadores, publicación de archivos y renderizado.

## Protocolo de rendimiento

El corpus registrará número de archivos, dimensiones, formatos y tamaño total. Se usarán al menos un conjunto pequeño, otro de 1.000 archivos y otro de 10.000, incluyendo imágenes de alta resolución dentro de los límites declarados.

Se anotarán hardware, sistema operativo, commit, build de release y configuración. Cada escenario se repetirá al menos cinco veces, registrando mediana, rango y memoria máxima observada. Las ejecuciones con caché fría y caliente se publicarán separadas; cualquier medición p95 requerirá suficientes muestras individuales.

| Escenario | Métricas |
| --- | --- |
| Inicio y reposo | Tiempo hasta ventana utilizable, memoria y CPU sin actividad |
| Abrir carpeta | Tiempo hasta primera cuadrícula y fin del escaneo |
| Navegar y ampliar | Latencia de previsualización, carga completa y respuesta de UI |
| Lote de exportación | Archivos por segundo, memoria máxima y respuesta del visor |
| Cambio y cancelación | Tiempo hasta nueva vista, liberación de reservas y salida segura |

Los valores de caché del diseño son presupuestos iniciales; los objetivos de latencia y memoria total se fijarán después del primer prototipo y quedarán registrados con su equipo de referencia. La ausencia de crecimiento sostenido al navegar repetidamente se comprobará sin exigir que el asignador devuelva inmediatamente toda la memoria al sistema.

Consultar la [arquitectura](architecture.md) para las responsabilidades y garantías de cada componente.
