# Observaciones de caché y navegación — 10 de octubre de 2026

- baseline-1000.csv y baseline-10000.csv: ejecutable measure del commit 90541ba, 5 repeticiones; instrumentación previa a la optimización.
- optimized-1000.csv y optimized-10000.csv: measure del commit 30e54a3, mismos corpus y opciones.
- navigation-release/cold-1.csv…cold-5.csv y warm-1.csv…warm-5.csv: 23 pasos por proceso, 230 observaciones medidas.
- navigation-release/prime-0.csv: preparación de la caché persistente, excluida de las estadísticas.
- navigation-release/process.csv: 10 procesos, memoria y CPU muestreadas.
- navigation-release/metadata.json: configuración nativa; únicamente se abrevian las dos rutas personales. Los CSV se copian sin transformación.
- builds.json: commits completos y SHA-256 de los tres ejecutables, toolchain, corpus y circunstancia de descargas durante el comienzo de la referencia.
- hardware.json: equipo consultado mediante CIM.
- summary.json: medianas, mínimos, máximos y comprobaciones de cobertura y huellas. Los campos terminados en _mib están en MiB; los CSV conservan los bytes originales.
- grid-photographs.png: captura real separada de los bancos, mismo ejecutable; 1.770×1.170 píxeles.

Para repetir los cálculos y validar las 144 copias y cuatro originales, con el corpus disponible:

    python scripts/summarize-performance.py docs/measurements/2026-10-10-cache C:\Fotos\Rastera-Pruebas-2026-10-10

La entrada de fotografías se lee; el script solo vuelve a escribir summary.json en el directorio de resultados. Las tablas de navegación usan medianas por recorrido antes de resumir las cinco repeticiones. Cinco repeticiones no bastan para p95 fiable.

Las fotografías y sus enlaces oficiales están en [photo-corpus.json](../../photo-corpus.json); el preparador es scripts/prepare-photo-corpus.py. Para ejecutar las mediciones, consultar [performance.md](../../performance.md). El [informe](../../performance-cache-results.md) explica el alcance de cada cifra.
