"""Bundle the illustrated tutorial and practice files for offline delivery."""
import base64
import mimetypes
import re
import sys
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

repo = Path(__file__).resolve().parent.parent
source = repo / "docs" / "tutorial"
destination = Path(sys.argv[1]).resolve()
destination.mkdir(parents=True, exist_ok=True)


def embed(match):
    relative = match.group(1)
    path = (source / relative).resolve()
    if not path.is_relative_to(source.resolve()):
        raise ValueError("Una imagen está fuera del tutorial")
    mime = mimetypes.guess_type(path.name)[0]
    encoded = base64.b64encode(path.read_bytes()).decode("ascii")
    return 'src="data:' + mime + ';base64,' + encoded + '"'


html = re.sub(r'src="([^"]+)"', embed, (source / "index.html").read_text(encoding="utf-8"))
standalone = destination / "rastera-tutorial.html"
standalone.write_text(html, encoding="utf-8")
instructions = """RASTERA · MATERIAL DE PRÁCTICA

1. Abre rastera-tutorial.html en tu navegador. Funciona sin conexión.
2. Abre Rastera y selecciona la carpeta Originales.
3. Sigue los casos 01 y 02 para seleccionar, marcar favoritos y etiquetar.
   Las etiquetas de las capturas pertenecen a un catálogo de demostración:
   no se importan automáticamente al abrir las fotos del paquete.
4. Exporta a la carpeta vacía Para-web con JPEG, calidad 90 y lado mayor 1000.
5. Copias-de-referencia contiene el resultado real del lote usado en la guía.
   No uses esa carpeta como destino: Rastera no reemplaza archivos existentes.

Fotos de ejemplo generadas con IA mediante image_gen integrado.
Procedencia y prompts: procedencia.json.
Capturas reales de Rastera 0.1.0 en Windows, 10 de octubre de 2026.
Los originales PNG miden 1254 x 1254 px; las copias JPEG, 1000 x 1000 px.
"""
with ZipFile(destination / "rastera-tutorial-y-ejemplos.zip", "w", ZIP_DEFLATED) as package:
    package.write(standalone, standalone.name)
    package.writestr("LEEME.txt", instructions)
    package.writestr("Para-web/", "")
    package.write(source / "procedencia.json", "procedencia.json")
    for path in sorted((source / "ejemplos").glob("*.png")):
        package.write(path, "Originales/" + path.name)
    for path in sorted((source / "copias").glob("*.jpg")):
        package.write(path, "Copias-de-referencia/" + path.name)
print(f"HTML sin conexión: {standalone.name} ({standalone.stat().st_size:,} bytes)")
print("Paquete: rastera-tutorial-y-ejemplos.zip")
