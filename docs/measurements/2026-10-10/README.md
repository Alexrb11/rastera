# Observaciones del 10 de octubre de 2026

Commit medido: 0aa3f4e56667992a0b0700ef4c590b5c29f92710. Consultar [resultados y límites](../../performance-results.md) y [metodología](../../performance.md).

- pipeline-1000.csv, pipeline-10000.csv y pipeline-detail.csv: 20 filas cada uno, cinco repeticiones de cuatro fases. Copias sin transformación.
- native-1000.csv, native-10000.csv y native-detail.csv: diez filas cada uno, cinco instancias frías y cinco calientes. Se excluye la instancia de preparación caliente.
- hardware.json: equipo registrado antes de las mediciones.
- panorama.png: entrada sintética del escenario de detalle, sin metadatos personales.

Los CSV nativos conservan todas las observaciones y campos. Las rutas se abrevian a `<repo>` y los números decimales se representan con punto en lugar de la coma de la configuración es-ES original. Los máximos privados proceden del muestreo; los de working set, del contador de Windows. Las columnas de CPU y duración están en milisegundos salvo las que indican porcentaje. Las de memoria están en bytes.

El máximo de reservas del banco es acumulado durante la repetición. Las filas preview y original de los corpus pequeños usan solo su primera imagen; no procesan la carpeta entera.

Para repetir el panorama, pasa esta imagen al visor y copia únicamente panorama.png a una carpeta nueva para el banco measure; este no debe recibir la carpeta de observaciones completa.
