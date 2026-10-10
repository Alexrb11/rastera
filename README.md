# Rastera

Visor y organizador de imágenes con herramientas por lotes, desarrollado en Rust.

Rastera tendrá una interfaz de escritorio nativa y trabajará sobre carpetas locales. El objetivo es navegar por bibliotecas grandes con una interfaz fluida, controlar el uso de memoria y procesar imágenes sin bloquear la navegación.

## Estado del proyecto

El repositorio contiene el diseño inicial. La aplicación todavía no está implementada y las decisiones de rendimiento deberán validarse con un prototipo.

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
