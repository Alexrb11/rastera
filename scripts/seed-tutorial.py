"""Create an isolated demonstration catalog; never edit a user's catalog."""
import sqlite3
import sys
from pathlib import Path

root, examples = (Path(arg).resolve() for arg in sys.argv[1:])
catalog = root / "Rastera" / "catalog.sqlite"
catalog.parent.mkdir(parents=True, exist_ok=True)
if catalog.exists():
    raise SystemExit("El catálogo de demostración ya existe.")
with sqlite3.connect(catalog) as db:
    db.executescript("""
        CREATE TABLE images (
            path BLOB PRIMARY KEY, favorite INTEGER NOT NULL DEFAULT 0,
            tags TEXT NOT NULL DEFAULT '', width INTEGER, height INTEGER, format TEXT
        );
        PRAGMA user_version=1;
    """)
    for name, favorite, tags in (
        ("01-taza.png", 1, "cerámica\naprobada"),
        ("02-cuenco.png", 1, "cerámica\naprobada"),
        ("03-jarron.png", 0, "cerámica\nrevisar"),
    ):
        path = str(examples / name).encode("utf-16-le")
        db.execute("INSERT INTO images(path,favorite,tags) VALUES(?,?,?)", (path, favorite, tags))
