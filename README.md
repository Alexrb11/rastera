<img src="assets/branding/rastera.svg" alt="Rastera" width="96" height="96">

# Rastera

**Español** | [English](README.en.md)

Visor y organizador de imágenes con herramientas por lotes, desarrollado en Rust.

Rastera tiene una interfaz de escritorio nativa y trabaja sobre carpetas locales. El objetivo es navegar por bibliotecas grandes con una interfaz fluida, controlar el uso de memoria y procesar imágenes sin bloquear la navegación.

## Probar Rastera

La primera versión es `0.1.0-alpha.2`, una alpha para Windows 10/11 x64. Las alphas publicadas se encuentran en [Releases](https://github.com/Alexrb11/rastera/releases). Descarga el ZIP portable y su SHA-256, extráelo y abre `rastera.exe`; no necesitas Rust. Consulta la [instalación y límites](docs/install-windows.md) y el [tutorial ilustrado](docs/tutorial/index.html).

El ejecutable Windows no está firmado. La prueba en un Windows limpio y la validación completa de accesibilidad están pendientes. El catálogo y la caché se guardan fuera de la carpeta del ejecutable, en `%LOCALAPPDATA%\Rastera`.

`main` prepara `0.1.0-alpha.3`, con el logo de Rastera y paquetes experimentales para Linux y macOS. Hasta publicar esa versión, `alpha.2` sigue siendo la descarga pública de referencia. Consulta las guías antes de probar un artefacto de Actions o una build propia:

| Plataforma | Paquete preparado para alpha.3 | Estado y requisitos |
| --- | --- | --- |
| Windows 10/11 x64 | ZIP portable MSVC | Plataforma inicial de la alpha; Windows limpio, otras GPU y accesibilidad pendientes. [Guía](docs/install-windows.md). |
| Linux x86-64 | tar.gz con instalador por usuario opcional | Experimental: glibc ≥ 2.35, Vulkan, X11/Wayland y portal de archivos. Pruebas en contenedores Ubuntu/Debian y revisión UI en X11 pasadas; hardware real y Wayland pendientes. [Guía](docs/install-linux.md). |
| macOS Apple Silicon | ZIP con `Rastera.app` arm64 | Experimental: mínimo declarado macOS 13, matriz de compilación macOS 15. Firma ad-hoc, sin Developer ID ni notarización. [Guía](docs/install-macos.md). |
| macOS Intel | ZIP con `Rastera.app` x64 | Mismos límites; pruebas de escritorio y del mínimo macOS 13 pendientes. [Guía](docs/install-macos.md). |

Linux usa los directorios XDG de datos y caché, con sus rutas de respaldo bajo HOME. macOS usa `Library/Application Support/Rastera` y `Library/Caches/Rastera`; un catálogo de una build anterior puede conservar su ubicación. Las guías detallan las rutas y la actualización. La interfaz y el tutorial siguen en español.

El paquete Linux alpha.3 pasó las 11 comprobaciones de ejecución en cada contenedor de Ubuntu 22.04, Ubuntu 24.04 y Debian 12 con Docker Desktop. La revisión manual de Ubuntu 24.04 en X11 confirmó cuadrícula, edición y persistencia de favoritos y etiquetas tras reiniciar, búsqueda, visor, actualización automática de carpetas, exportación JPEG, rechazo de colisiones, diálogo GTK y cierre normal. Estas pruebas usan Vulkan por software sobre WSL2; no validan GPU físicas, Wayland ni macOS. Consulta [las pruebas en contenedores](docs/testing-linux-containers.md) para conocer la revisión y el hash del paquete, los resultados y los límites.

## Estado del proyecto

El primer visor está implementado: abre JPEG, PNG y WebP estáticos mediante diálogo, ruta, arrastre o argumento de inicio. Incluye zoom, desplazamiento, orientación EXIF, transparencia y detalles de la imagen. La carga se ejecuta en un trabajador con cancelación y presupuesto de memoria.

La cuadrícula dibuja únicamente las filas visibles, carga miniaturas de 256 píxeles y ofrece selección múltiple con Ctrl+clic (Cmd+clic en macOS), selección de toda la carpeta y apertura con doble clic. El botón Actualizar vuelve a leer la carpeta e invalida las texturas. Su caché LRU mantiene como máximo 128 texturas (32 MiB de píxeles en GPU), además de la vista previa. Las miniaturas se reutilizan entre sesiones mediante una caché desechable en disco. El visor tiene preferencia al solicitar buffers; los trabajadores comparten el presupuesto de memoria.

El catálogo SQLite guarda favoritos, etiquetas y dimensiones de las imágenes abiertas. Organizar selección aplica favoritos y añade o retira etiquetas para varias imágenes en una transacción, conservando los datos particulares de cada una. La cuadrícula permite buscar por nombre o etiqueta y filtrar por favoritos y formato. Los lotes convierten a PNG, JPEG o WebP sin pérdidas, redimensionan sin ampliar y rotan en pasos de 90 grados. Incluyen revisión del plan, progreso, cancelación y diario SQLite por lote. Exportan desde los originales a otra carpeta y publican sin reemplazar archivos existentes. Hay [mediciones iniciales](docs/performance-results.md) de caché, tiempos y memoria en Windows con corpus sintéticos.

También se pueden abrir carpetas mediante diálogo, ruta, arrastre o argumento de inicio. La lectura emite páginas progresivamente y la lista dibuja únicamente las filas visibles. Las flechas permiten pasar al archivo anterior o siguiente. El alcance inicial es una carpeta sin recursión, con un límite de 10.000 archivos y 16 MiB de rutas; los enlaces se excluyen. La lista se ordena por ruta al terminar el escaneo. Una extensión compatible no garantiza que su contenido sea una imagen válida.

La carpeta abierta se actualiza automáticamente al añadir, modificar, renombrar o retirar archivos. Los cambios se agrupan en segundo plano y una revisión cada 15 segundos recupera eventos perdidos. Se conservan los filtros, la selección de rutas presentes y las texturas sin cambios. Un escaneo incompleto conserva la biblioteca anterior y muestra el error mientras reintenta. Las rutas renombradas se consideran nuevas y no heredan favoritos ni etiquetas.

Windows fue la primera plataforma de desarrollo. Linux y macOS comparten visor, catálogo, observación y lotes; la integración de escritorio y los paquetes se validan por separado en cada sistema. Pasar pruebas en CI no sustituye comprobar una instalación de usuario.

## Funciones

- Interfaz de gestor de imágenes con barras compactas, selección por rango y panel de propiedades plegable.
- Navegación por carpetas con cuadrícula de miniaturas, selección y ordenación.
- Actualización automática de la carpeta abierta, recarga del visor y recuperación de eventos perdidos.
- Visor con zoom, desplazamiento, ajuste a ventana y navegación con teclado.
- Catálogo local con favoritos, etiquetas, búsqueda y organización conjunta de la selección.
- Navegación de la cuadrícula con flechas, páginas, rangos con Shift y apertura con Enter.
- Conversión, redimensionado y rotación de imágenes por lotes.
- Revisión de nombres y dimensiones del lote, progreso, cancelación, resultados y recuperación de pendientes con huellas verificadas.
- Renombrado por lotes en una fase posterior, con registro de operaciones y recuperación.

El primer alcance cubre JPEG, PNG y WebP estáticos SDR de 8 bits. Visor, miniaturas y lotes convierten perfiles ICC RGB compatibles de matriz y curvas a sRGB. Se rechazan HDR, CMYK, perfiles LUT y declaraciones PNG de color no compatibles. La compatibilidad se comprueba mediante archivos de prueba; otros formatos y la gestión avanzada de color se incorporarán por separado.

## Base técnica

La arquitectura es un workspace de Cargo con cuatro componentes: escritorio, dominio, motor de aplicación y adaptadores. La interfaz usa `egui/eframe`, el catálogo SQLite y el procesamiento se ejecuta en trabajadores con colas y memoria limitadas.

El diseño favorece la carga progresiva y la reutilización de recursos. Usar Rust no constituye, por sí mismo, una garantía de menor consumo que otras aplicaciones.

## Documentación

- [Guía de uso](docs/usage.md): selección, organización y exportación.
- [Instalación Linux](docs/install-linux.md) y [macOS](docs/install-macos.md): requisitos, paquetes experimentales, directorios y límites de integración; guías en inglés.
- [Pruebas Linux en contenedores](docs/testing-linux-containers.md): ejecución sin Rust en Ubuntu/Debian con Docker Desktop, informes y escritorio opcional para revisión manual; guía en inglés.
- [Tutorial ilustrado](docs/tutorial/index.html): casos de uso paso a paso, capturas reales e imágenes de práctica. Ábrelo en un navegador; [preparación y procedencia](docs/tutorial/README.md).
- [Arquitectura](docs/architecture.md): módulos, dependencias, concurrencia, catálogo, cachés y operaciones sobre archivos.
- [Plan de desarrollo](docs/roadmap.md): entregas y criterios de aceptación.
- [Medir rendimiento](docs/performance.md): contadores, banco reproducible y medición del proceso.
- [Resultados iniciales](docs/performance-results.md): equipo, escenarios, observaciones y límites de la comparación.
- [Optimización de caché y navegación](docs/performance-cache-results.md): comparación antes/después y recorridos nativos con fotografías reales.
- [Preparar una release](docs/releasing.md): paquetes de escritorio, comprobaciones y recorrido manual en cada plataforma.

La inspiración de producto es el enfoque de aplicaciones nativas y locales de [Artcraft](https://getartcraft.com/apps). Rastera tendrá su propio alcance y diseño.

## Colaborar y licencia

El código de Rastera usa [MIT](LICENSE). Dependencias y fuentes conservan sus propios términos, incluidos en cada paquete y documentados en [licensing](licensing/README.md). Para colaborar consulta [CONTRIBUTING.md](CONTRIBUTING.md), el [código de conducta](CODE_OF_CONDUCT.md) y la [política de seguridad](SECURITY.md). Las incidencias reproducibles y las pruebas con distintos equipos Windows, escritorios Linux y Macs son especialmente útiles durante la alpha.

## Desarrollo

Se requiere Rust 1.95 o posterior y las herramientas de compilación del sistema. En Windows, el toolchain MSVC requiere las herramientas de C++ y el SDK de Windows; GNU requiere un entorno MinGW compatible. La validación local inicial usa Rust 1.99 sobre Windows GNU.

Linux necesita las bibliotecas de desarrollo y los servicios de escritorio de [su guía](docs/install-linux.md); macOS, las Xcode Command Line Tools. En ambos puedes ejecutar `cargo run -p rastera-desktop --locked -- "$HOME/Pictures"`. El empaquetado se hace en el sistema destino; consulta [la preparación de releases](docs/releasing.md).

```powershell
cargo run -p rastera-desktop --locked
cargo run -p rastera-desktop --locked -- "C:\Fotos\imagen.png"
```

En PowerShell también se puede usar `.\scripts\dev.ps1 run`. El script aprovecha un toolchain local en `work/` cuando exista y Cargo no esté disponible en el PATH; las herramientas locales no forman parte del repositorio.

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

La primera compilación descargará dependencias y tardará más que las siguientes. El visor muestra primero una vista previa de hasta 4.096 píxeles y carga resolución completa al ampliar, con mosaicos visibles y memoria acotada. 100% corresponde a píxeles físicos de pantalla. La conversión al perfil del monitor sigue pendiente. Los límites iniciales son 128 MiB por archivo, 64 millones de píxeles y un presupuesto compartido de buffers de 512 MiB. Una imagen puede rechazarse por memoria aunque cumpla el límite de píxeles. Este presupuesto no equivale a la memoria total del proceso.

La característica opcional `capture` permite una comprobación visual del renderizado real: con `RASTERA_CAPTURE_TO` apuntando a un PNG, la aplicación guarda una captura cuando termina la carga y se cierra. `RASTERA_CAPTURE_SIZE=760x480` prueba la ventana mínima; `RASTERA_CAPTURE_SELECTION=1` selecciona las primeras tres entradas y `RASTERA_CAPTURE_FILTER` permite comprobar filtros sin coincidencias. Es una herramienta de desarrollo, deshabilitada por defecto.
