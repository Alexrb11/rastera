# Primeras mediciones de Rastera

Esta referencia histórica se complementa con la [comparación posterior de caché y navegación con fotografías](performance-cache-results.md), que mide y reduce el coste del mantenimiento señalado al final.

Fecha: 10 de octubre de 2026. Versión medida: 0.1.0, commit 0aa3f4e56667992a0b0700ef4c590b5c29f92710, sin cambios locales durante las ejecuciones. Esta es una referencia inicial de procesamiento y memoria; no compara Rastera con otros visores.

## Equipo y escenarios

- Windows 11 Pro 10.0.26200; Intel Core i5-1240P, 12 núcleos y 16 procesadores lógicos; 16.469.048 KiB de RAM visible.
- Intel Iris Xe Graphics, controlador 32.0.101.7026; renderizado WGPU.
- Rust 1.99, Windows GNU; perfil release con thin LTO y una unidad de generación de código. El ejecutable nativo incluye la característica de desarrollo capture.
- Ventana de 1.180×780 puntos, capturas de 1.770×1.170 píxeles, escalado 150%.
- Cinco repeticiones por escenario. Se muestran mediana y rango mínimo–máximo. Las cifras usan punto decimal; MiB significa 1.048.576 bytes.

Los corpus de carpetas tienen 1.000 y 10.000 PNG RGB de 64×48, idénticos en contenido y distintos en ruta. Cada archivo ocupa 3.265 bytes: 3.265.000 y 32.650.000 bytes en total. SHA-256 del contenido: 4797067511650667cd279d663dce2385b5714b290bcb8bd96d12c8f42c459fa8.

El escenario de detalle usa un PNG RGBA de 9.000×1.000 con degradado, patrón y alfa: 256.408 bytes. Se conserva como [panorama.png](measurements/2026-10-10/panorama.png); SHA-256: b3a52229b67d4199ec90f790b84592df0976bf6705ef916800ca4ce6165248ea.

La [metodología y comandos](performance.md) describen los dos bancos. Caché fría significa caché de Rastera vacía; la caché del sistema operativo no se purga. Las fases y escenarios se ejecutaron secuencialmente, sin compilaciones concurrentes iniciadas por este banco.

## Procesamiento sin interfaz

El banco solicita todas las miniaturas una tras otra mediante los trabajadores reales. Estas cifras no son tiempos de apertura de la cuadrícula, que solicita solo las miniaturas visibles.

| Corpus | Escaneo completo, ms | Primera página, ms | Miniaturas frías, s | Miniaturas calientes, s |
| --- | --- | --- | --- | --- |
| 1.000 | 1.669 (1.485–1.769) | 0.168 (0.146–0.218) | 2.034 (1.712–2.072) | 0.482 (0.477–0.536) |
| 10.000 | 12.874 (11.844–15.630) | 0.231 (0.189–0.285) | 41.058 (36.629–46.301) | 25.388 (24.616–27.274) |

En todas las repeticiones, cada fase fría registró una decodificación solicitada por archivo y cero aciertos; cada fase caliente, un acierto por archivo y cero decodificaciones. El máximo acumulado de reservas en estos corpus fue 49.152 bytes y la reserva tras liberar cada resultado terminó en cero.

| PNG de 9 megapíxeles | Duración, ms |
| --- | --- |
| Miniatura con caché fría | 34.233 (29.608–35.283) |
| Miniatura con caché caliente | 5.923 (4.721–14.941) |
| Vista previa de hasta 4.096 píxeles | 59.339 (58.091–64.677) |
| Resolución completa | 19.634 (19.187–20.470) |

El máximo acumulado de reservas fue 144.000.000 bytes (137.329 MiB), compartido por las fases de cada repetición; no implica que el acierto de miniatura requiriese esa memoria. Tras liberar resultados, las reservas volvieron a cero. Son estimaciones de buffers, no RAM total. En este PNG, la vista previa añade redimensionado y tarda más que el original; los tiempos excluyen GPU y presentación.

## Proceso nativo de Windows

Cada instancia se observó durante unos diez segundos, con muestreo cada 100 ms. La fase caliente tuvo una preparación excluida y datos compartidos entre sus cinco instancias; solo se calientan las miniaturas solicitadas por la vista. El visor de detalle se inició a 100%, conservando la vista previa y el original.

| Escena y caché | Máximo working set, MiB | Working set final, MiB | Máximo privado muestreado, MiB |
| --- | --- | --- | --- |
| Cuadrícula 1.000, fría | 348.984 (348.844–368.332) | 340.980 (340.418–359.625) | 277.070 (276.762–279.098) |
| Cuadrícula 1.000, caliente | 348.891 (348.699–348.938) | 340.559 (340.109–342.633) | 276.902 (276.781–278.930) |
| Cuadrícula 10.000, fría | 349.617 (349.168–351.016) | 345.668 (344.582–348.602) | 284.000 (283.223–285.863) |
| Cuadrícula 10.000, caliente | 348.984 (348.836–350.996) | 344.766 (344.324–347.395) | 284.074 (283.211–285.895) |
| Detalle 9 MP, fría | 383.812 (383.039–389.977) | 376.781 (374.293–376.879) | 315.715 (313.309–317.031) |
| Detalle 9 MP, caliente | 383.742 (383.676–389.422) | 376.316 (375.996–377.160) | 315.246 (313.422–316.797) |

| Escena y caché | CPU total del proceso, ms | CPU en últimos dos segundos, % de un núcleo |
| --- | --- | --- |
| Cuadrícula 1.000, fría | 281.250 (203.125–375.000) | 0.000 (0.000–0.000) |
| Cuadrícula 1.000, caliente | 234.375 (187.500–328.125) | 0.000 (0.000–0.766) |
| Cuadrícula 10.000, fría | 421.875 (375.000–500.000) | 0.000 (0.000–0.000) |
| Cuadrícula 10.000, caliente | 437.500 (421.875–500.000) | 1.507 (0.000–2.429) |
| Detalle 9 MP, fría | 265.625 (250.000–343.750) | 0.000 (0.000–1.593) |
| Detalle 9 MP, caliente | 296.875 (265.625–359.375) | 0.000 (0.000–1.523) |

La CPU total es tiempo acumulado de ejecución, no tiempo hasta presentar la escena. El CSV incluye también el porcentaje dividido entre los 16 procesadores lógicos. Cero significa que el contador no registró incremento en el intervalo; no demuestra ausencia absoluta de trabajo.

Una comprobación visual separada del mismo build confirmó la cuadrícula de 10.000 archivos con 25 miniaturas cargadas y el original con detalle completo. En este último, el panel mostró 68.7 MiB de reservas actuales, máximo 137.3 MiB, 7.1 MiB de píxeles de vista previa y 7.9 MiB de mosaicos. Son estimaciones internas; no deben sumarse al working set como si fueran costes independientes. La comprobación no instrumentó el instante de finalización de cada una de las 30 ejecuciones.

## Interpretación y siguiente medición

La caché reutiliza todas las miniaturas en estos corpus, pero el coste de procesar 10.000 crece más que el número de archivos. El código recorre el directorio de caché periódicamente; perfilar ese mantenimiento es un candidato concreto a optimización, todavía sin una atribución causal medida.

La memoria de las dos cuadrículas es parecida en una vista fija, coherente con cargar solo las filas visibles. El proceso sigue consumiendo cientos de MiB y la gestión gráfica tiene costes que el presupuesto de buffers no describe. No se ha medido crecimiento tras desplazarse repetidamente.

Los corpus son sintéticos y fáciles de comprimir. Las ventanas se iniciaron en modo oculto, lo que puede influir en la planificación gráfica. No se midieron VRAM real, primera presentación, FPS, latencia de entrada, exportaciones ni cancelación bajo carga. Cinco observaciones permiten esta mediana y rango, no p95 fiable. La siguiente comparación debe incluir fotografías variadas JPEG/WebP, ICC, desplazamiento, zoom y un lote concurrente.

Las [observaciones CSV y hardware](measurements/2026-10-10/README.md) permiten revisar los cálculos. Se conservaron todas las filas; en los CSV del proceso solo se abreviaron rutas personales a `<repo>` y se normalizó el separador decimal a punto. Los CSV del banco se copiaron sin transformación.
