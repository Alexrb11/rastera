# Contribuir a Rastera

Rastera es una aplicación local de escritorio en Rust. La primera alpha se dirige a Windows x64; agradecemos pruebas con fotografías variadas, mejoras de accesibilidad y correcciones reproducibles.

## Preparar el entorno

Instala Rust 1.95 o posterior. En Windows, usa el toolchain MSVC con las herramientas de C++ y el SDK de Windows, o GNU con MinGW compatible. En Linux se necesitan las dependencias nativas indicadas en `.github/workflows/ci.yml`; su distribución aún no está validada.

```powershell
cargo run -p rastera-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
```

Lee [la arquitectura](docs/architecture.md) y [la guía de uso](docs/usage.md). El dominio no depende de egui ni SQLite. El acceso a disco y la decodificación deben ejecutarse fuera del hilo gráfico; conserva cancelación, generaciones, presupuestos y límites de cola.

## Proponer cambios

Para una funcionalidad grande, abre primero una propuesta explicando el problema y un caso concreto de uso. Para un fallo, incluye versión, Windows y GPU, pasos, resultado esperado y resultado observado. Usa imágenes de prueba que puedas compartir y oculta rutas personales y metadatos sensibles.

Trabaja en una rama propia. Agrupa los commits por funcionalidad y escribe los mensajes en inglés, tanto el área como la descripción: `feat(area): description`, `fix(area): description` o `docs(area): description`. Por ejemplo: `feat(docs): add English README`. Una pull request debe explicar el comportamiento resultante y cómo se comprobó. Añade pruebas de comportamiento para cambios en archivos, catálogo, memoria o concurrencia. Para cambios visuales, incluye capturas de ventana grande y compacta.

No incluyas `target/`, `work/`, catálogos personales, credenciales ni fotografías ajenas. No añadas llamadas de red al flujo de uso ni cambios sobre originales sin un diseño explícito. Los trabajadores de exportación publican en otra carpeta y conservan las salidas existentes.

Las contribuciones al código y documentación se ofrecen bajo la licencia MIT del proyecto. Las dependencias y fuentes incorporadas conservan sus propias licencias. Respeta [el código de conducta](CODE_OF_CONDUCT.md). Los problemas de seguridad se comunican mediante [SECURITY.md](SECURITY.md).
