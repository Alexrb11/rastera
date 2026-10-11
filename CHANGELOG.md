# Cambios

## 0.1.0-alpha.2 · 2026-10-11

Primera alpha publicada. La alpha.1 se conserva como borrador de preparación.

- MSVC enlaza el runtime C estáticamente para evitar la dependencia de VCRUNTIME140.dll.
- El empaquetado revisa imports directos y diferidos del PE y rechaza DLL externas sin resolver.
- Los ZIP incorporan los avisos de la biblioteca estándar de Rust y, en GNU, del runtime MinGW-w64.
- Se conservan las funciones y los límites descritos en alpha.1. Windows limpio y accesibilidad siguen pendientes.

## 0.1.0-alpha.1 · 2026-10-11

Primera alpha para Windows x64.

- Visor JPEG, PNG y WebP estáticos con orientación EXIF, transparencia, zoom, desplazamiento y detalle a resolución completa.
- Biblioteca con cuadrícula virtualizada, miniaturas persistentes, selección múltiple, navegación por teclado y filtros.
- Catálogo SQLite con favoritos, etiquetas y organización conjunta en una transacción.
- Actualización automática de la carpeta abierta y revisión periódica de respaldo.
- Exportación de copias PNG, JPEG y WebP, redimensionado y rotación desde originales, revisión del plan, cancelación y recuperación mediante diarios.
- Paquete portable, licencia MIT, avisos de terceros, guía ilustrada y archivos para colaborar.

Alcance: una carpeta sin recursión y hasta 10.000 entradas. Los renombrados externos no trasladan la organización. La conversión al perfil del monitor, validación completa de accesibilidad, búsqueda global y otras plataformas siguen pendientes. Consulta [instalación y límites](docs/install-windows.md).
