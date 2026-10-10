# Rastera

Visor y organizador de imágenes con herramientas por lotes, desarrollado en Rust.

Rastera tendrá una interfaz de escritorio nativa y trabajará sobre carpetas locales. El objetivo es navegar por bibliotecas grandes con una interfaz fluida, controlar el uso de memoria y procesar imágenes sin bloquear la navegación.

## Estado del proyecto

El primer visor está implementado: abre JPEG, PNG y WebP estáticos mediante diálogo, ruta, arrastre o argumento de inicio. Incluye zoom, desplazamiento, orientación EXIF, transparencia y detalles de la imagen. La carga se ejecuta en un trabajador con cancelación y presupuesto de memoria.

La cuadrícula dibuja únicamente las filas visibles, carga miniaturas de 256 píxeles y ofrece selección múltiple con Ctrl+clic, selección de toda la carpeta y apertura con doble clic. El botón Actualizar vuelve a leer la carpeta e invalida las miniaturas. Su caché LRU mantiene como máximo 128 texturas (32 MiB de píxeles en GPU), además de la vista previa. El visor tiene preferencia al solicitar buffers; los trabajadores comparten el presupuesto de memoria. La caché todavía no se guarda en disco.

El catálogo y las operaciones por lotes están pendientes. No hay una comparación de rendimiento publicada.

También se pueden abrir carpetas mediante diálogo, ruta, arrastre o argumento de inicio. La lectura emite páginas progresivamente y la lista dibuja únicamente las filas visibles. Las flechas permiten pasar al archivo anterior o siguiente. El alcance inicial es una carpeta sin recursión, con un límite de 10.000 archivos y 16 MiB de rutas; los enlaces se excluyen. La lista se ordena por ruta al terminar el escaneo. Una extensión compatible no garantiza que su contenido sea una imagen válida.

Windows será la primera plataforma de desarrollo. La separación entre interfaz, lógica y adaptadores permitirá incorporar Linux y macOS después de comprobar su integración y distribución.

## Funciones previstas

- Navegación por carpetas con cuadrícula de miniaturas, selección y ordenación.
- Visor con zoom, desplazamiento, ajuste a ventana y navegación con teclado.
- Catálogo local con favoritos, etiquetas y búsqueda por nombre y metadatos básicos.
- Conversión, redimensionado y rotación de imágenes por lotes.
- Vista previa del lote, progreso, cancelación y resultados por archivo.
- Renombrado por lotes en una fase posterior, con registro de operaciones y recuperación.

El primer alcance cubrirá JPEG, PNG y WebP estáticos. La compatibilidad se comprobará mediante archivos de prueba; otros formatos y la gestión avanzada de color se incorporarán por separado.

## Base técnica

La arquitectura prevista es un workspace de Cargo con cuatro componentes: escritorio, dominio, motor de aplicación y adaptadores. La interfaz usará `egui/eframe`, el catálogo SQLite y el procesamiento se ejecutará en trabajadores con colas y memoria limitadas.

El diseño favorece la carga progresiva y la reutilización de recursos. Usar Rust no constituye, por sí mismo, una garantía de menor consumo que otras aplicaciones.

## Documentación

- [Arquitectura](docs/architecture.md): módulos, dependencias, concurrencia, catálogo, cachés y operaciones sobre archivos.
- [Plan de desarrollo](docs/roadmap.md): entregas y criterios de aceptación.

La inspiración de producto es el enfoque de aplicaciones nativas y locales de [Artcraft](https://getartcraft.com/apps). Rastera tendrá su propio alcance y diseño.

## Desarrollo

Se requiere Rust 1.95 o posterior y las herramientas de compilación del sistema. En Windows, el toolchain MSVC requiere las herramientas de C++ y el SDK de Windows; GNU requiere un entorno MinGW compatible. La validación local inicial usa Rust 1.99 sobre Windows GNU.

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

La primera compilación descargará dependencias y tardará más que las siguientes. El visor limita la vista previa a 4.096 píxeles por lado. El zoom se aplica sobre esa vista; la carga completa bajo demanda y la gestión de color ICC siguen pendientes. Los límites iniciales son 128 MiB por archivo, 64 millones de píxeles y un presupuesto compartido de buffers de 512 MiB. Una imagen puede rechazarse por memoria aunque cumpla el límite de píxeles. Este presupuesto no equivale a la memoria total del proceso.

La característica opcional `capture` permite una comprobación visual del renderizado real: con `RASTERA_CAPTURE_TO` apuntando a un PNG, la aplicación guarda una captura cuando termina la carga y se cierra. Es una herramienta de desarrollo, deshabilitada por defecto.
