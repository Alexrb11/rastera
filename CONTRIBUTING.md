# Contribuir a Rastera

Rastera es una aplicación local de escritorio en Rust. La alpha inicial se dirige a Windows x64; se prepara soporte experimental para Linux x86-64 y macOS Intel/Apple Silicon. Agradecemos pruebas con fotografías variadas, integración de escritorio, mejoras de accesibilidad y correcciones reproducibles.

## Preparar el entorno

Instala Rust 1.95 o posterior. En Windows, usa el toolchain MSVC con las herramientas de C++ y el SDK de Windows, o GNU con MinGW compatible. En Linux, instala un toolchain C/C++, pkg-config y las [dependencias de desarrollo y escritorio](docs/install-linux.md). En macOS, usa las Xcode Command Line Tools y revisa [los requisitos e integración](docs/install-macos.md). La matriz de CI prepara Windows, Ubuntu 22.04, macOS 15 ARM y macOS 15 Intel; un runner no valida los diálogos ni permisos de un escritorio real.

```powershell
cargo run -p rastera-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

Lee [la arquitectura](docs/architecture.md) y [la guía de uso](docs/usage.md). El dominio no depende de egui ni SQLite. El acceso a disco y la decodificación deben ejecutarse fuera del hilo gráfico; conserva cancelación, generaciones, presupuestos y límites de cola.

Para comprobar un paquete Linux desde Windows con Docker Desktop, consulta [las pruebas en contenedores](docs/testing-linux-containers.md). Incluyen Ubuntu 22.04, Ubuntu 24.04 y Debian 12 como entornos de ejecución sin Rust, un workflow manual de Actions y un escritorio opcional en el navegador. La guía registra 11 comprobaciones pasadas por distribución para alpha.3 y revisión UI independiente en Ubuntu 24.04, incluida exportación JPEG, rechazo de colisiones y persistencia de favoritos y etiquetas tras reiniciar. Conserva la revisión del paquete, informes y límites de software Vulkan/X11/WSL2 al comunicar resultados; la persistencia con fixtures no demuestra edición desde la UI, ni estos contenedores validan macOS.

## Proponer cambios

Para una funcionalidad grande, abre primero una propuesta explicando el problema y un caso concreto de uso. Para un fallo, incluye versión, sistema operativo, arquitectura y GPU/controlador, pasos, resultado esperado y resultado observado. En Linux añade escritorio y X11/Wayland; en macOS, versión y chip/CPU. Usa imágenes de prueba que puedas compartir y oculta rutas personales y metadatos sensibles.

Trabaja en una rama propia. Agrupa los commits por funcionalidad y escribe los mensajes en inglés, tanto el área como la descripción: `feat(area): description`, `fix(area): description` o `docs(area): description`. Por ejemplo: `feat(docs): add English README`. Una pull request debe explicar el comportamiento resultante y cómo se comprobó. Añade pruebas de comportamiento para cambios en archivos, catálogo, memoria o concurrencia. Para cambios visuales, incluye capturas de ventana grande y compacta.

No incluyas `target/`, `work/`, catálogos personales, credenciales ni fotografías ajenas. No añadas llamadas de red al flujo de uso ni cambios sobre originales sin un diseño explícito. Los trabajadores de exportación publican en otra carpeta y conservan las salidas existentes.

Los directorios de datos se resuelven en el adaptador compartido; no introduzcas rutas específicas del sistema en la UI. Usa `Modifiers::COMMAND` para acciones que deban funcionar con Ctrl en Windows/Linux y Cmd en macOS, y conserva las etiquetas apropiadas para cada plataforma. Las pruebas usan perfiles temporales. Comprueba archivos con Unicode y permisos, observación de carpetas, colisiones de exportación y reinicio antes de declarar soporte de escritorio.

Las contribuciones al código y documentación se ofrecen bajo la licencia MIT del proyecto. Las dependencias y fuentes incorporadas conservan sus propias licencias. Respeta [el código de conducta](CODE_OF_CONDUCT.md). Los problemas de seguridad se comunican mediante [SECURITY.md](SECURITY.md).
