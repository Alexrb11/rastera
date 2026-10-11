# Licencias de terceros

Rastera usa MIT. Sus dependencias y las fuentes integradas mantienen los términos de sus respectivos autores. Las fotografías de práctica generadas con IA tienen su procedencia documentada en `docs/tutorial/procedencia.json`; el paquete no incorpora Segoe UI, que se lee del sistema cuando está disponible.

`scripts/third_party_notices.py` toma `cargo metadata --locked --format-version 1 --filter-platform <target>` y recorre las dependencias normales y de compilación del escritorio. Excluye las aristas exclusivas de pruebas. Recoge textos de licencia, copyright y avisos de los paquetes descargados y conserva su suma SHA-256 en un inventario. Los textos recogidos pueden abarcar también componentes que el upstream distribuye pero esta build no utiliza.

Las expresiones SPDX aceptadas están enumeradas en el script. Una expresión nueva, un texto ausente o una revisión de upstream distinta impiden crear el paquete hasta revisarlo. Para `Apache-2.0 OR GPL-2.0-only` se utiliza la opción Apache-2.0; las obligaciones AND se conservan, incluidas Unicode y las fuentes OFL y Ubuntu. No se relicencian esos recursos bajo MIT.

Algunos paquetes omiten sus textos de licencia en el archivo de crates.io. `upstream-sources.json` registra las URL fijadas al commit de `.cargo_vcs_info.json`, archivos y huellas recuperados del repositorio oficial. Los textos se conservan aquí para generar releases sin descargarlos durante el empaquetado. Cuando una dependencia cambie de revisión, se debe actualizar y revisar su procedencia.

El inventario se genera por separado para Windows, Linux y macOS (Intel o Apple Silicon), a partir del grafo filtrado para cada target. Se excluyen archivos de código como `copying.rs`: describen APIs y no constituyen textos de licencia.

Algunas revisiones de `objc2` solo incluyen una declaración `LICENSE.md` que enlaza los términos MIT, Apache o Zlib; `dispatch` 0.2.0 declara MIT en su `Cargo.toml` sin conservar un texto completo. Para esos paquetes se mantiene la declaración exacta del commit y un mapping explícito `license_template: MIT` complementa sus avisos con el texto canónico de MIT de SPDX. `templates/sources.json` fija la versión, commit, URL y SHA-256 de ese texto. Los placeholders de año y titulares se conservan literalmente, sin inventar atribuciones. Los autores declarados en Cargo se incluyen por separado y no se presentan como titulares de copyright. Una plantilla o mapping nuevos, una expresión no revisada o una huella distinta bloquean el empaquetado. No se descarga ningún texto al generar paquetes.

Cada paquete incluye `LICENSE`, `THIRD-PARTY-NOTICES.txt` y `dependency-licenses.json`. El inventario conserva declaraciones de upstream y textos de términos; es una comprobación de procedencia y cobertura de avisos, no una auditoría legal completa de todo el código fuente ni de las condiciones del SDK de cada plataforma.

Los runtimes de compilación se documentan en [toolchain](toolchain/README.md). El ZIP conserva los avisos de la biblioteca estándar del compilador utilizado y de MinGW-w64 en GNU.
