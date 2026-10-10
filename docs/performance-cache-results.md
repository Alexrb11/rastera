# Mantenimiento de caché y navegación con fotografías

10 de octubre de 2026. La mediana del procesamiento secuencial de 10.000 miniaturas sintéticas con caché caliente baja de 26,121 s a 1,613 s: 16,20 veces más rápido en este equipo. La nueva navegación nativa completó 230 pasos sin errores con fotografías reales. Son bancos diferentes: la comparación de 10.000 archivos no mide apertura de la cuadrícula ni interacción manual.

## Qué cambió

La caché antes enumeraba el directorio cada 64 peticiones y ordenaba las entradas aunque no hubiera que expulsarlas. Ahora realiza un inventario inicial, otro tras acumular 16 MiB de escrituras y una reconciliación al atender una petición después de 60 segundos. Solo ordena las entradas cuando supera la cuota. Leer miniaturas guardadas deja de provocar recorridos por número de peticiones. La enumeración y eliminación comprueban cancelación.

Se mantiene la comprobación del original, clave completa, checksum, regeneración de registros dañados y carga desde el original ante fallos de caché. La cuota de 2 GiB es un objetivo con margen de 16 MiB más un registro por trabajador; varias instancias no comparten un límite estricto.

## Comparación del procesamiento

Cinco repeticiones por versión y corpus; mediana y rango mínimo–máximo. Mismos directorios de 1.000/10.000 PNG RGB de 64×48, contenido idéntico y rutas distintas. Caché fría de Rastera vacía, caliente creada por esa misma repetición; sin purgar la caché del SO. No son fotografías.

| Archivos y caché | Antes, s | Después, s | Cociente de medianas |
| --- | --- | --- | --- |
| 1.000 · fría | 1.404 (1.398–2.329) | 1.430 (1.345–1.985) | 0.98× |
| 1.000 · caliente | 0.420 (0.406–0.463) | 0.135 (0.129–0.202) | 3.12× |
| 10.000 · fría | 38.888 (32.240–45.576) | 21.808 (20.911–23.420) | 1.78× |
| 10.000 · caliente | 26.121 (24.755–29.774) | 1.613 (1.563–1.745) | 16.20× |

En la referencia caliente de 10.000 archivos hubo 156 recorridos y 1.560.000 entradas examinadas por repetición. El mantenimiento consumió una mediana de 24,897 s, alrededor del 95 % del tiempo total. Después hubo cero recorridos en esa fase, 10.000 aciertos y cero decodificaciones solicitadas en las cinco repeticiones. Se conserva el inventario inicial y la reconciliación por tiempo en cada nueva sesión.

En frío, los recorridos de 10.000 archivos bajaron de 157 a 8; las entradas examinadas, de 783.744 a 37.240. El tiempo mediano de mantenimiento pasó de 16,770 s a 1,068 s. No hubo expulsiones en ninguno de estos corpus. La mediana fría de 1.000 archivos quedó prácticamente igual; esta optimización tiene más efecto cuando la enumeración repetida domina el coste. Las reservas estimadas alcanzaron 49.152 bytes en el banco sintético y volvieron a cero al liberar los resultados.

## Navegación nativa con fotografías

Se creó en Imágenes la carpeta Rastera-Pruebas-2026-10-10. Contiene cuatro originales JPEG públicos CC0 de Wikimedia Commons y 144 copias físicas sin recodificar: 533.137.068 bytes, fotografías de 4,3–12 MP, todas con ICC. Son cuatro fotos diferentes, no 144; este conjunto permite ejercitar la caché de texturas y tiene poca diversidad. Los [autores, fuentes, licencias y SHA-256](photo-corpus.json) permiten comprobar su procedencia. Se verificaron las huellas de los cuatro originales y las 144 copias después de las pruebas.

El recorrido avanza por 0, 20, 40… y el último índice, vuelve hasta el primero y abre las seis primeras imágenes en el visor ajustado. Son nueve pasos hacia delante, ocho de regreso y seis del visor. En cada uno se espera a todas las miniaturas del intervalo virtualizado, incluidas las filas de margen. Cerca del final algunos pasos comparten parte o todo el intervalo. Cada repetición cubrió los 144 archivos y mantuvo un máximo de 128 entradas de miniaturas residentes. Las cinco frías solicitaron 144 decodificaciones y recuperaron 16 entradas del disco al volver; las calientes registraron 160 aciertos y cero decodificaciones solicitadas.

Se ejecutaron cinco procesos fríos y cinco calientes; una preparación completa queda excluida. Las calientes reutilizan el disco, pero cada proceso crea sus propias texturas. El regreso reutiliza las texturas que siguen residentes. Datos aislados del catálogo personal, ventana iniciada en modo oculto de 1.180×780 puntos, WGPU, escalado 150 %. No se ejecutaron compilaciones ni otros bancos del agente durante estos recorridos.

Los tiempos son hasta disponibilidad en la UI, antes de presentación. Para avance, regreso y visor se calcula primero la mediana de los pasos de cada recorrido y después la mediana y rango de los cinco recorridos. El primer paso se muestra por separado. Las escenas ya residentes pueden quedar disponibles en la misma llamada a la UI y registrar menos de 1 ms: esto no es latencia de entrada ni de presentación.

| Escena o llamada | Fría, ms | Caliente, ms |
| --- | --- | --- |
| Primera cuadrícula | 3296.357 (3175.744–4089.685) | 520.452 (519.589–534.394) |
| Avance por la cuadrícula | 2441.593 (2391.820–2467.127) | 333.006 (332.558–333.458) |
| Regreso por la cuadrícula | 0.435 (0.401–0.670) | 0.396 (0.363–0.428) |
| Abrir vista previa en el visor | 108.160 (104.010–114.868) | 107.101 (105.263–107.678) |
| Mayor duración de una llamada a App::ui por recorrido | 15.291 (14.248–19.487) | 13.435 (13.001–14.666) |

El primer tiempo comienza al construir la aplicación; los siguientes, antes de cambiar la escena dentro de App::ui. La duración de App::ui incluye recepción y preparación de texturas, pero excluye el renderizado posterior y la espera del bucle de eventos. No son medidas de FPS. Abrir el visor vuelve a decodificar la imagen en ambas condiciones; la caché persistente medida aquí guarda miniaturas.

| Recursos del recorrido | Fría | Caliente |
| --- | --- | --- |
| Duración del proceso, s | 19.568 (19.041–20.754) | 4.064 (4.045–4.097) |
| CPU acumulada muestreada, s | 18.453 (18.062–19.219) | 1.516 (1.484–1.609) |
| Máximo working set observado, MiB | 437.656 (434.973–438.570) | 439.531 (437.820–439.828) |
| Máximo privado muestreado, MiB | 359.301 (357.570–360.441) | 336.230 (315.430–349.609) |
| Máximo de reservas de buffers, MiB | 183.105 (183.105–183.105) | 183.105 (183.105–183.105) |

El proceso sigue consumiendo cientos de MiB. Las reservas estiman buffers y no limitan toda la RAM; las 128 miniaturas no representan VRAM total. Los picos de proceso y el tiempo final de CPU se observan cada 100 ms y pueden perder intervalos breves. El máximo de reservas es acumulado por recorrido. La observación de una ida y vuelta no demuestra ausencia de crecimiento en sesiones largas.

Una [captura nativa separada](measurements/2026-10-10-cache/grid-photographs.png) del mismo ejecutable confirma las fotografías en la cuadrícula y sus propiedades. No se utiliza para medir los tiempos anteriores.

## Alcance y validación

Equipo: Windows 11 Pro 10.0.26200, Intel Core i5-1240P de 12 núcleos/16 procesadores lógicos, 16.469.048 KiB de RAM visible, Iris Xe con controlador 32.0.101.7026. Rust 1.99, Windows GNU, release con thin LTO. Referencia 90541ba; versión optimizada y navegación 30e54a3, sin cambios locales durante las mediciones. La característica capture es de desarrollo; la compilación normal no incorpora el recorrido automático.

Durante el inicio de la referencia se descargaron dos fotos para preparar el corpus. No se compiló ni se ejecutó otro banco del agente en paralelo. Se conserva esa circunstancia en los metadatos; esta comparación en un solo equipo y sin purgar el SO no establece resultados universales.

Pasaron formato, Clippy sin advertencias, compilación normal y 49 pruebas del workspace con todas las características. La prueba nueva verifica que 200 peticiones no vuelven a inventariar la caché y que la revisión por tiempo elimina registros añadidos externamente sin tocar archivos ajenos. Se corrigió además una comparación de ICC que fallaba al cambiar el segundo de creación del perfil, manteniendo la comprobación de los demás bytes y de la transformación de color. Las pruebas existentes cubren corrupción, cambio del original, cuota pequeña, cancelación y caché inaccesible.

El banco de navegación comprueba disponibilidad de cada escena, errores, cobertura y límites. No mide teclado/ratón, primera presentación, VRAM real, HDR/RAW, lotes concurrentes ni navegación prolongada. Faltan corpus con más fotografías diferentes, WebP y perfiles variados. No se ha comparado la navegación nativa antes/después de esta optimización; sus cifras constituyen una nueva referencia.

En el código actual la cuadrícula solicita una miniatura cada vez y programa la siguiente desde la UI. La cadencia observada en caliente sugiere estudiar esa entrega como siguiente coste; no se ha aislado aún su contribución mediante otra comparación.

Los [CSV completos, hardware, ejecutables identificados y resumen reproducible](measurements/2026-10-10-cache/README.md) conservan todas las observaciones. Los [comandos y definiciones](performance.md) describen cómo repetir los bancos.
