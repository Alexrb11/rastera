# Runtimes de compilación

El ZIP incorpora `RUST-STANDARD-LIBRARY-NOTICES.html` desde el sysroot del compilador utilizado, con su SHA-256 en BUILD-INFO.json. Se instala con `rustup component add rust-docs`. Si el toolchain no separa los avisos de la biblioteca, se conserva COPYRIGHT.html completo.

GNU incluye `COPYING.MinGW-w64-runtime.txt`, conservado sin conversión de fin de línea desde la distribución w64devkit empleada en la build local. SHA-256: `ecff91ddb5799c8b79a4bdb576921b9c3e2989783fd1944527e8eb9b41a5ff5f`. Procedencia: [w64devkit](https://github.com/skeeto/w64devkit), archivo del mismo nombre; corresponde a avisos para distribuir binarios que incorporan el runtime [MinGW-w64](https://github.com/mingw-w64/mingw-w64).

La build MSVC de distribución utiliza `-C target-feature=+crt-static` con target explícito, siguiendo [la referencia de Rust](https://doc.rust-lang.org/reference/linkage.html#static-and-dynamic-c-runtimes). La lista de imports PE se conserva en BUILD-INFO.json. La biblioteca C del compilador y los componentes del sistema mantienen los términos de sus proveedores; la licencia MIT de Rastera no los relicencia. Estos avisos complementan el inventario de Cargo y no constituyen una auditoría completa del toolchain.
