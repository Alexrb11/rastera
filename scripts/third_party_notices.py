"""Collect notices for the selected desktop target graph, without network access."""
import argparse
import hashlib
import json
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
# Reviewed expressions in the Windows, Linux and macOS graphs. New expressions stop packaging.
REVIEWED = {
    "MIT", "Apache-2.0", "MIT OR Apache-2.0", "Apache-2.0 OR MIT",
    "MIT/Apache-2.0", "MIT / Apache-2.0", "Apache-2.0/MIT", "Apache-2.0 AND MIT",
    "MIT OR Apache-2.0 OR Zlib", "Zlib OR Apache-2.0 OR MIT",
    "MIT OR Zlib OR Apache-2.0", "Unlicense OR MIT", "Unlicense/MIT",
    "BSD-2-Clause OR Apache-2.0 OR MIT", "BSD-3-Clause OR Apache-2.0",
    "BSL-1.0", "Zlib", "CC0-1.0", "ISC", "0BSD OR MIT OR Apache-2.0",
    "Apache-2.0 OR GPL-2.0-only", "CC0-1.0 OR MIT-0 OR Apache-2.0",
    "CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception",
    "Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT",
    "(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0",
    "(MIT OR Apache-2.0) AND Unicode-3.0",
}


def normal_graph(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = next(p["id"] for p in packages.values() if p["name"] == "rastera-desktop")
    pending, seen = [root], set()
    while pending:
        key = pending.pop()
        if key in seen:
            continue
        seen.add(key)
        pending.extend(d["pkg"] for d in nodes[key]["deps"] if any(k["kind"] != "dev" for k in d["dep_kinds"]))
    return sorted((packages[key] for key in seen if packages[key]["source"]), key=lambda p: (p["name"], p["version"]))


def license_file(path):
    # Source files such as Foundation's copying.rs describe APIs, not licenses.
    if path.suffix.lower() in {".rs", ".c", ".h", ".cpp", ".hpp", ".m", ".mm", ".py", ".js", ".ts"}:
        return False
    return path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice")) or path.name.lower() in {"ofl.txt", "ufl.txt"}


def collect(metadata_path, output):
    metadata = json.loads(metadata_path.read_text(encoding="utf-8-sig"))
    provenance = json.loads((REPO / "licensing/upstream-sources.json").read_text(encoding="utf-8"))
    for source in provenance:
        data = (REPO / "licensing" / source["file"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != source["sha256"]:
            raise ValueError("Changed upstream notice: " + source["file"])
    templates = json.loads((REPO / "licensing/templates/sources.json").read_text(encoding="utf-8"))
    template_sources = {}
    for source in templates:
        if source["license"] != "MIT" or source["license"] in template_sources:
            raise ValueError("Unreviewed or duplicate supplemental license template")
        data = (REPO / "licensing" / source["file"]).read_bytes()
        if hashlib.sha256(data).hexdigest() != source["sha256"]:
            raise ValueError("Changed supplemental license template: " + source["file"])
        template_sources[source["license"]] = source
    sections = ["RASTERA · THIRD-PARTY NOTICES\n\nRastera is MIT licensed. The dependencies and embedded fonts below retain their own terms.\nThis inventory includes normal and build dependencies for the selected desktop target, excluding dev-only dependencies.\nFor Apache-2.0 OR GPL-2.0-only, distribution uses Apache-2.0.\nAll applicable AND terms, including OFL, Ubuntu font and Unicode licenses, remain in effect.\nSome upstream archives omit notices; their exact-commit texts are preserved in licensing/upstream-sources.json.\n"]
    inventory = []
    for package in normal_graph(metadata):
        expression = package["license"]
        if expression not in REVIEWED:
            raise ValueError(f"Unreviewed license: {package['name']} {expression}")
        root = Path(package["manifest_path"]).parent
        texts = [(str(p.relative_to(root)).replace("\\", "/"), p.read_bytes()) for p in sorted(root.rglob("*")) if p.is_file() and license_file(p)]
        extra = [s for s in provenance if s["package"] == package["name"] and s["version"] == package["version"]]
        vcs = root / ".cargo_vcs_info.json"
        revision = json.loads(vcs.read_text())["git"]["sha1"] if vcs.exists() else None
        if extra and any(f"/{revision}/" not in s["url"] for s in extra):
            raise ValueError("Upstream revision changed: " + package["name"])
        texts.extend((s["url"], (REPO / "licensing" / s["file"]).read_bytes()) for s in extra)
        supplemental = {s["license_template"] for s in extra if "license_template" in s}
        for license_id in sorted(supplemental):
            # These mappings are explicitly reviewed for upstream declarations
            # that omit a full text. Do not infer terms or copyright holders.
            if license_id != "MIT" or expression not in {"MIT", "Zlib OR Apache-2.0 OR MIT"}:
                raise ValueError("Unreviewed supplemental license mapping: " + package["name"])
            source = template_sources[license_id]
            label = ("Supplemental canonical MIT text; upstream declares MIT but omits its full text. "
                     "SPDX placeholders are retained and do not identify this package's copyright holders. "
                     + source["url"])
            texts.append((label, (REPO / "licensing" / source["file"]).read_bytes()))
        if not texts:
            raise ValueError("Missing license text: " + package["name"])
        # Include embedded font terms even though this crate's own code has MIT/Apache terms.
        if package["name"] == "epaint_default_fonts" and not {"fonts/OFL.txt", "fonts/UFL.txt"}.issubset({name for name, _ in texts}):
            raise ValueError("Missing embedded font notices")
        sections.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\nSPDX: {expression}\nRepository: {package.get('repository') or 'not declared'}\nDeclared authors (Cargo metadata, not inferred copyright): {', '.join(package.get('authors', [])) or 'not declared'}\n")
        records = []
        for name, data in texts:
            sections.append(f"\n--- {name} ---\n" + data.decode("utf-8", errors="replace") + "\n")
            records.append({"source": name, "sha256": hashlib.sha256(data).hexdigest()})
        inventory.append({"name": package["name"], "version": package["version"], "license": expression, "repository": package.get("repository"), "authors": package.get("authors", []), "supplemental_license_templates": sorted(supplemental), "notices": records})
    output.mkdir(parents=True, exist_ok=True)
    (output / "THIRD-PARTY-NOTICES.txt").write_text("".join(sections), encoding="utf-8")
    (output / "dependency-licenses.json").write_text(json.dumps(inventory, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"Collected notices for {len(inventory)} packages")


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("metadata", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    collect(args.metadata, args.output)
