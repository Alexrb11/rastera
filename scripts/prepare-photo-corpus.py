"""Download the four attributed CC0 originals and copy them without recoding."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import urllib.request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path, help="New directory; never an existing collection")
    parser.add_argument("--copies", type=int, default=144)
    args = parser.parse_args()
    if not 1 <= args.copies <= 10_000:
        parser.error("copies must be between 1 and 10000")
    manifest_path = Path(__file__).resolve().parents[1] / "docs" / "photo-corpus.json"
    manifest = json.loads(manifest_path.read_text(encoding="utf-8-sig"))
    args.destination.mkdir(parents=True, exist_ok=False)
    originals = args.destination / "Originales"
    originals.mkdir()
    for photo in manifest["photos"]:
        request = urllib.request.Request(photo["download"], headers={
            "User-Agent": "RasteraPhotoBenchmark/0.1 (four CC0 originals; no automatic retries)"
        })
        target = originals / photo["file"]
        with urllib.request.urlopen(request, timeout=90) as response, target.open("xb") as output:
            shutil.copyfileobj(response, output)
        digest = hashlib.sha256(target.read_bytes()).hexdigest()
        if target.stat().st_size != photo["bytes"] or digest != photo["sha256"]:
            raise ValueError(f"Source changed: {target}; inspect the partial directory")
    copies = args.destination / f"Navegacion-{args.copies}"
    copies.mkdir()
    for index in range(args.copies):
        photo = manifest["photos"][index % len(manifest["photos"])]
        shutil.copyfile(originals / photo["file"], copies / f"foto-{index:04}.jpg")
    manifest["files"] = args.copies
    manifest["corpus"] = f"{args.copies} copias físicas sin recodificar, distribuidas cíclicamente por nombre"
    (args.destination / "procedencia.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (args.destination / "LEEME.txt").write_text(
        f"Fotografías reales públicas CC0 de Wikimedia Commons. Ver procedencia.json. "
        f"Originales contiene {len(manifest['photos'])} fotos distintas; Navegacion-{args.copies} "
        f"repite sus bytes en {args.copies} rutas para ejercitar la cuadrícula y la expulsión "
        "de texturas. No representa ese número de fotos distintas.\n", encoding="utf-8"
    )
    print(args.destination.resolve())


if __name__ == "__main__":
    main()
