import argparse
import csv
import hashlib
import json
from pathlib import Path
from statistics import median

parser = argparse.ArgumentParser(description="Recompute and validate the recorded cache/navigation comparison")
parser.add_argument("results", type=Path)
parser.add_argument("photos", type=Path)
args = parser.parse_args()
root = args.results.resolve()


def read(path):
    with path.open(encoding="utf-8-sig", newline="") as source:
        return list(csv.DictReader(source))


def summary(values):
    return {"median": median(values), "min": min(values), "max": max(values)}


results = {"pipeline": {}, "navigation": {}, "validation": {}}
for size in [1000, 10000]:
    for version in ["baseline", "optimized"]:
        rows = read(root / f"{version}-{size}.csv")
        assert len(rows) == 20
        for phase in ["cold", "warm"]:
            selected = [row for row in rows if row["phase"] == phase]
            assert len(selected) == 5
            assert all(int(row["files"]) == size and int(row["reserved_after_bytes"]) == 0 for row in selected)
            assert all(int(row["cache_hits"]) == (size if phase == "warm" else 0) for row in selected)
            assert all(int(row["cache_misses"]) == (size if phase == "cold" else 0) for row in selected)
            results["pipeline"][f"{version}-{size}-{phase}"] = {
                field: summary([float(row[field]) for row in selected])
                for field in ["elapsed_ms", "maintenance_ms", "maintenance_runs", "maintenance_files"]
            }

nav = root / "navigation-release"
prime = read(nav / "prime-0.csv")
assert len(prime) == 23 and all(row["status"] == "ready" and row["errors"] == "0" for row in prime)
for mode in ["cold", "warm"]:
    values = {key: [] for key in ["first_grid", "grid_forward", "grid_reverse", "viewer", "ui_max_ms", "ui_after_first_max_ms", "peak_reserved_mib", "textures", "cache_hits", "cache_misses", "maintenance_ms", "maintenance_runs"]}
    for run in range(1, 6):
        rows = read(nav / f"{mode}-{run}.csv")
        assert len(rows) == 23
        assert all(row["status"] == "ready" and row["errors"] == "0" for row in rows)
        forward = [row for row in rows if row["scene"] == "grid_forward"]
        covered = set()
        for row in forward:
            assert int(row["ready"]) == int(row["visible"])
            first, last, target = map(int, [row["visible_first"], row["visible_last"], row["requested_index"]])
            assert first <= target < last
            covered.update(range(first, last))
        assert covered == set(range(144))
        assert max(int(row["thumbnail_textures"]) for row in rows) == 128
        assert max(int(row["reserved_peak_bytes"]) for row in rows) <= 512 * 1024 * 1024
        assert sum(int(row["cache_hits"]) for row in rows) == (16 if mode == "cold" else 160)
        assert sum(int(row["cache_misses"]) for row in rows) == (144 if mode == "cold" else 0)
        values["first_grid"].append(float(rows[0]["ready_ms"]))
        for scene in ["grid_forward", "grid_reverse", "viewer"]:
            selected = [row for row in rows if row["scene"] == scene and row["step"] != "1"]
            assert len(selected) == (6 if scene == "viewer" else 8)
            values[scene].append(median(float(row["ready_ms"]) for row in selected))
        values["ui_max_ms"].append(max(float(row["ui_max_ms"]) for row in rows))
        values["ui_after_first_max_ms"].append(max(float(row["ui_max_ms"]) for row in rows[1:]))
        values["peak_reserved_mib"].append(max(int(row["reserved_peak_bytes"]) for row in rows) / 1048576)
        values["textures"].append(max(int(row["thumbnail_textures"]) for row in rows))
        for key in ["cache_hits", "cache_misses", "maintenance_ms", "maintenance_runs"]:
            values[key].append(sum(float(row[key]) for row in rows))
    results["navigation"][mode] = {key: summary(group) for key, group in values.items()}

process = read(nav / "process.csv")
assert len(process) == 10
for mode in ["cold", "warm"]:
    for field in ["duration_ms", "sampled_cpu_ms", "peak_working_set_bytes", "sampled_peak_private_bytes"]:
        values = [float(row[field]) for row in process if row["cache"] == mode]
        if field.endswith("bytes"):
            values = [value / 1048576 for value in values]
        results["navigation"][mode][field.replace("_bytes", "_mib")] = summary(values)

photo_root = args.photos.resolve()
manifest = json.loads((photo_root / "procedencia.json").read_text(encoding="utf-8-sig"))
hashes = {photo["sha256"]: photo["bytes"] for photo in manifest["photos"]}
for photo in manifest["photos"]:
    source = photo_root / "Originales" / photo["file"]
    assert hashlib.sha256(source.read_bytes()).hexdigest() == photo["sha256"]
copies = sorted((photo_root / "Navegacion-144").glob("*.jpg"))
assert len(copies) == 144
for copy in copies:
    digest = hashlib.sha256(copy.read_bytes()).hexdigest()
    assert digest in hashes and copy.stat().st_size == hashes[digest]
results["validation"] = {"navigation_runs": 10, "steps": 230, "files_covered_per_run": 144, "copy_hashes_checked": 144, "original_hashes_checked": 4}
(root / "summary.json").write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")
print(json.dumps(results, indent=2))
