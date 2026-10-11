# Seguridad

## Versiones atendidas

Durante la alpha se atenderán correcciones en la versión preliminar más reciente. No hay todavía una rama estable con soporte prolongado ni plazos de respuesta garantizados.

## Comunicar una vulnerabilidad

Si GitHub ofrece «Report a vulnerability» en la pestaña Security de este repositorio, usa ese formulario privado. Si no está habilitado, abre una incidencia que solicite un canal privado, sin publicar el archivo de explotación ni detalles que faciliten reproducir el ataque. El mantenedor coordinará el canal y la corrección.

Incluye versión, sistema, pasos de reproducción, impacto y un archivo mínimo que tengas permiso para compartir. Evita fotografías privadas, tokens y rutas personales. No uses la plantilla pública de fallos para publicar una vulnerabilidad sin coordinar.

## Alcance

Rastera procesa imágenes locales y no requiere cuenta, telemetría ni conexión durante su uso. Los códecs y dependencias procesan contenido externo. La alpha no ejecuta los decodificadores en un sandbox separado. La exportación usa una carpeta distinta y publicación sin sobrescritura; la aplicación todavía no ofrece operaciones de borrado o renombrado de originales.

Las builds públicas normales no incluyen la característica de desarrollo `capture`. Una suma SHA-256 permite comprobar integridad, pero no sustituye una firma del editor. Descarga los binarios desde las releases del repositorio oficial.

Windows se distribuye sin firma de editor. Los bundles experimentales de macOS tienen una firma ad-hoc y no cuentan con Developer ID ni notarización. No se solicita desactivar Gatekeeper, retirar atributos de cuarentena ni conceder Full Disk Access por defecto; la [guía de macOS](https://github.com/Alexrb11/rastera/blob/main/docs/install-macos.md) enlaza el procedimiento de excepción individual de Apple para una copia cuya procedencia haya verificado el usuario. Las inspecciones del paquete y sus dependencias no equivalen a una auditoría de seguridad, y el soporte experimental de Linux/macOS no implica que se hayan validado todos sus escritorios, permisos o versiones mínimas.
