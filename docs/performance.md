# Medir recursos y tiempos

Herramientas → Diagnóstico de rendimiento abre contadores locales de la sesión: reservas CPU actuales, máximo y límite; tamaños de píxeles GPU; cargas completadas, fallidas y canceladas; duración de la última carga; aciertos y decodificaciones solicitadas por la caché en disco.

Los [resultados iniciales del 10 de octubre de 2026](performance-results.md) incluyen las observaciones CSV y el equipo de referencia.

Las reservas estiman buffers y entrega, no toda la RAM. Los tamaños GPU excluyen copias y sobrecostes del controlador. La duración del trabajador incluye inspección, espera de memoria, decodificación y color hasta encolar el resultado; no mide la presentación en pantalla. No se transmiten datos fuera de la aplicación.

## Banco del procesamiento

El ejemplo measure usa los adaptadores y trabajadores reales, con colas y reservas. Genera un corpus sintético en una carpeta nueva y guarda un CSV nuevo, sin reemplazar archivos. En cada repetición mide el escaneo, el procesamiento secuencial de todas las miniaturas con caché fría y caliente, y la vista previa y resolución completa de la primera imagen.

    New-Item -ItemType Directory -Force work/measurements
    cargo build -p rastera-adapters --example measure --release --locked
    .\target\release\examples\measure.exe --generate work/measurements/corpus-1000 1000
    .\target\release\examples\measure.exe work/measurements/corpus-1000 work/measurements/pipeline-1000.csv 5

El corpus generado contiene PNG RGB de 64×48 con contenido idéntico y rutas distintas. Sirve para medir enumeración, costes por archivo y reutilización de caché; no representa fotografías, JPEG/WebP, variedad de perfiles, ni rendimiento de exportaciones. Se puede pasar una carpeta propia para otros corpus dentro de los límites del programa.

Cada repetición dispone de una caché temporal exclusiva que se elimina al terminar. La fase fría significa caché de Rastera vacía; no se purga la caché del sistema operativo. La fase caliente utiliza las entradas creadas por la fase fría. El CSV registra aciertos, fallos de caché, máximo de reservas y reservas tras liberar resultados. Los fallos de lectura detienen la medición. El escaneo y su primera página se miden directamente, sin catálogo, GPU ni UI.

El banco procesa todas las imágenes de forma secuencial. La cuadrícula de la aplicación procesa únicamente las visibles, así que el tiempo de este banco no es el tiempo de apertura de la ventana ni de primera cuadrícula.

## Proceso nativo en Windows

El script measure-window inicia instancias de prueba con directorios de datos aislados, toma muestras cada 100 ms y cierra cada instancia. La caché fría usa datos nuevos por ejecución; la caliente tiene una ejecución de preparación excluida y comparte datos entre las siguientes. Los directorios de diagnóstico se conservan junto al CSV para inspección.

    cargo build -p rastera-desktop --release --features capture --locked
    .\scripts\measure-window.ps1 -InputPath work/measurements/corpus-1000 -OutputPath work/measurements/native-1000.csv -Runs 5 -Seconds 10
    .\scripts\measure-window.ps1 -InputPath C:\Fotos\panorama.png -OutputPath work/measurements/native-detail.csv -Runs 5 -Seconds 10 -Full

Full activa 100% mediante la característica de desarrollo capture y comprueba que el ejecutable la incluye. No solicita una captura. Sin Full se usa la vista habitual de la carpeta o imagen. El script restaura las variables de entorno y nunca utiliza el catálogo personal.

El CSV del proceso diferencia working set (memoria residente) y memoria privada muestreada; las reservas internas se miden por separado en el banco y el panel. El máximo de working set procede del SO; el máximo privado depende del muestreo y puede perder picos breves. La CPU total incluye arranque y procesamiento; la CPU final corresponde a los últimos dos segundos, expresada como porcentaje de un núcleo y de la capacidad lógica del equipo. No demuestra que la aplicación hubiera terminado de cargar: comprueba la escena con capture antes de interpretar esa fase como reposo.

Los números del CSV usan punto decimal independientemente de la configuración regional. En el banco, reserved_peak_bytes es el máximo acumulado de toda la repetición, compartido por sus cuatro fases; no es un máximo independiente de cada fila.

Se ejecuta con ventana iniciada en modo oculto. Esto puede influir en la planificación gráfica y no representa interacción continua. No mide VRAM real, latencia de entrada, primera presentación, rendimiento de lotes ni respuesta durante desplazamiento.

## Registrar una comparación

Anota fecha, commit, cambios sin confirmar, build y características, toolchain, CPU, GPU/controlador, RAM, SO, corpus y estado de caché. Usa al menos cinco repeticiones y publica mediana y rango, conservando los CSV originales. No deduzcas p95 de cinco observaciones ni compares cifras obtenidas con diferentes corpus o escenarios.
