"""Verify a Unix release archive, its native dependencies and optional Linux GUI startup."""
import argparse
import json
import math
import os
import re
import shutil
import sqlite3
import stat
import subprocess
import tarfile
import tempfile
import time
from pathlib import Path
from zipfile import ZipFile

from package_unix_support import TARGETS, architecture, check_bundle, dependencies, native_target, relative_path, sha256

MAX_EXTRACTED_BYTES = 2 * 1024 * 1024 * 1024
MAX_ENTRIES = 10000


def safe_mode(mode, directory):
    if mode & (0o7000 | 0o022):
        raise ValueError("Unsafe package permissions")
    expected = 0o755 if directory or mode & 0o111 else 0o644
    if stat.S_IMODE(mode) != expected:
        raise ValueError("Expected normalized 644/755 package permissions")
    return expected


def safe_extract(archive, destination):
    """Validate the entire archive before extracting only ordinary files/directories."""
    destination = Path(destination)
    entries, names, total = [], set(), 0
    is_zip = archive.name.endswith(".zip")
    stream = ZipFile(archive) if is_zip else tarfile.open(archive, "r:gz")
    try:
        for member in stream.infolist() if is_zip else stream.getmembers():
            name = member.filename.rstrip("/") if is_zip else member.name.rstrip("/")
            path = relative_path(name)
            if name in names:
                raise ValueError("Duplicate archive path")
            names.add(name)
            if is_zip:
                directory = member.is_dir()
                mode = member.external_attr >> 16
                kind = stat.S_IFMT(mode)
                if kind not in ({stat.S_IFDIR} if directory else {stat.S_IFREG}):
                    raise ValueError("ZIP contains a link or special file")
                size = member.file_size
                timestamp = time.mktime((*member.date_time, 0, 0, -1))
            else:
                directory = member.isdir()
                if not directory and not member.isreg():
                    raise ValueError("TAR contains a link or special file")
                mode = member.mode
                size = member.size
                timestamp = member.mtime
            mode = safe_mode(mode, directory)
            if size < 0 or not math.isfinite(timestamp) or not 0 <= timestamp <= 4102444800:
                raise ValueError("Invalid archive size or timestamp")
            total += size
            entries.append((member, path, directory, mode, timestamp))
            if total > MAX_EXTRACTED_BYTES or len(entries) > MAX_ENTRIES:
                raise ValueError("Archive exceeds release extraction limits")
        # Reject file/parent collisions before writing anything.
        files = {path for _, path, directory, _, _ in entries if not directory}
        if any(parent in files for _, path, _, _, _ in entries for parent in path.parents if str(parent) != "."):
            raise ValueError("A package file is also a parent directory")
        roots = {path.parts[0] for _, path, _, _, _ in entries}
        if len(roots) != 1:
            raise ValueError("Expected exactly one application folder")
        if not destination.is_dir() or any(destination.iterdir()):
            raise ValueError("Extract into a new, empty directory")
        for member, path, directory, mode, timestamp in entries:
            target = destination.joinpath(*path.parts)
            if directory:
                target.mkdir(parents=True, exist_ok=True)
            else:
                target.parent.mkdir(parents=True, exist_ok=True)
                source = stream.open(member) if is_zip else stream.extractfile(member)
                with source, target.open("xb") as output:
                    shutil.copyfileobj(source, output)
            target.chmod(mode)
            os.utime(target, (timestamp, timestamp))
        # Implicit ZIP directories inherit an explicit, portable mode too.
        for path in destination.rglob("*"):
            if path.is_dir():
                path.chmod(0o755)
        return destination / roots.pop()
    finally:
        stream.close()


def isolated_environment(root):
    env = dict(os.environ)
    for key in list(env):
        if key.startswith("DYLD_") or key in {"LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT"}:
            env.pop(key)
    env["HOME"] = str(root / "home")
    env["XDG_DATA_HOME"] = str(root / "data")
    env["XDG_CACHE_HOME"] = str(root / "cache")
    env["XDG_CONFIG_HOME"] = str(root / "config")
    env["XDG_RUNTIME_DIR"] = str(root / "runtime")
    for name in ("HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR"):
        Path(env[name]).mkdir(parents=True, exist_ok=True, mode=0o700)
        Path(env[name]).chmod(0o700)
    return env


def smoke_linux(executable, package, documents, env, root):
    sample = next(iter(sorted((documents / "docs/tutorial").rglob("*.jpg"))), None)
    if not sample:
        raise ValueError("The package must include a sample JPEG for the GUI smoke test")
    catalog = Path(env["XDG_DATA_HOME"]) / "Rastera/catalog.sqlite"
    log_path = root / "gui-smoke.log"
    with log_path.open("w", encoding="utf-8") as log:
        process = subprocess.Popen([str(executable), str(sample)], cwd=package, env=env, stdout=log, stderr=log)
        try:
            deadline = time.monotonic() + 45
            decoded = None
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise ValueError("The GUI exited during startup:\n" + log_path.read_text(errors="replace")[-6000:])
                if catalog.is_file():
                    try:
                        with sqlite3.connect("file:" + catalog.as_posix() + "?mode=ro", uri=True) as database:
                            decoded = database.execute("SELECT width, height, format FROM images WHERE width > 0 AND height > 0 LIMIT 1").fetchone()
                    except sqlite3.Error:
                        pass
                if decoded:
                    time.sleep(2)
                    if process.poll() is not None:
                        raise ValueError("The GUI exited after opening the sample image")
                    return {"opened_sample": str(sample.relative_to(package)), "decoded_image": {"width": decoded[0], "height": decoded[1], "format": decoded[2]}, "catalog_created": True}
                time.sleep(0.2)
            raise ValueError("The GUI did not decode and catalogue the sample JPEG within 45 seconds")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)


def verify(archive, smoke=False, inspect_only=False, expected_commit=None):
    archive = archive.resolve()
    checksum = Path(str(archive) + ".sha256").read_text(encoding="ascii").strip()
    match = re.fullmatch(r"([a-fA-F0-9]{64})  (.+)", checksum)
    if not match or match[2] != archive.name or match[1].lower() != sha256(archive):
        raise ValueError("The archive does not match its SHA-256 sidecar")
    if smoke and inspect_only:
        raise ValueError("--smoke and --inspect-only cannot be combined")
    report = {"archive": archive.name, "archive_sha256": match[1].lower(), "scope": "Archive integrity, package metadata, architecture and permissions", "native_checked": False, "gui_checked": False}
    with tempfile.TemporaryDirectory(prefix="rastera-verify-") as folder:
        root = Path(folder)
        extracted = root / "extracted"
        extracted.mkdir()
        package = safe_extract(archive, extracted)
        info = json.loads((package / "BUILD-INFO.json").read_text(encoding="utf-8"))
        if info.get("capture") is not False or info.get("target") not in TARGETS or not re.fullmatch(r"[a-f0-9]{40}", info.get("source_commit", "")) or not re.fullmatch(r"\d+\.\d+\.\d+(?:-alpha\.\d+)?", info.get("version", "")):
            raise ValueError("Invalid normal-release build metadata")
        if expected_commit and info["source_commit"] != expected_commit:
            raise ValueError("The archive source does not match the expected commit")
        platform_name, arch = TARGETS[info["target"]]
        expected_name = f"rastera-{info['version']}-{platform_name}-{arch}"
        extension = ".tar.gz" if platform_name == "linux" else ".zip"
        if package.name != expected_name or archive.name != expected_name + extension:
            raise ValueError("The archive filename does not match its build metadata")
        expected_binary = "bin/rastera" if platform_name == "linux" else "Rastera.app/Contents/MacOS/rastera"
        expected_documents = "." if platform_name == "linux" else "Rastera.app/Contents/Resources"
        if info.get("executable") != expected_binary or info.get("documentation_root") != expected_documents:
            raise ValueError("Unexpected binary/documentation layout")
        executable = package.joinpath(*relative_path(expected_binary).parts)
        documents = package if expected_documents == "." else package.joinpath(*relative_path(expected_documents).parts)
        required = ("LICENSE", "CHANGELOG.md", "SECURITY.md", "README.md", "THIRD-PARTY-NOTICES.txt", "RUST-STANDARD-LIBRARY-NOTICES.html", "dependency-licenses.json", "docs/usage.md", "docs/tutorial/index.html", "docs/tutorial/procedencia.json", "assets/branding/rastera.svg", "assets/branding/rastera-32.png")
        for name in required:
            if not (documents / name).is_file():
                raise ValueError("Missing package file: " + name)
        if sha256(executable) != info["executable_sha256"] or sha256(documents / "RUST-STANDARD-LIBRARY-NOTICES.html") != info["rust_library_notices_sha256"]:
            raise ValueError("The executable or Rust notice digest does not match")
        if not json.loads((documents / "dependency-licenses.json").read_text()):
            raise ValueError("The dependency notice inventory is empty")
        actual_arch = architecture(executable, info["target"])
        if actual_arch != info.get("architecture"):
            raise ValueError("The architecture metadata is incorrect")
        if platform_name == "macos":
            bundle = check_bundle(package, info["version"])
            if info.get("signing") != "ad-hoc; no Developer ID or notarization":
                raise ValueError("Expected an ad-hoc signed development bundle")
        else:
            for file in ("install-user.sh", "install-user.py", "share/applications/rastera.desktop"):
                if not (package / file).is_file():
                    raise ValueError("Missing Linux integration file: " + file)
        if os.name != "nt" and stat.S_IMODE(executable.stat().st_mode) != 0o755:
            raise ValueError("The extracted application is not executable")
        report.update({"version": info["version"], "target": info["target"], "source_commit": info["source_commit"], "architecture": actual_arch})
        if not inspect_only:
            native_target(info["target"])
            host_dependencies = dependencies(executable, info["target"])
            expected_dependencies = info.get("dependencies", {})
            if platform_name == "linux":
                names = lambda deps: {item["name"] for item in deps["system_libraries"]}
                if names(host_dependencies) != names(expected_dependencies) or host_dependencies["maximum_required_glibc"] != expected_dependencies.get("maximum_required_glibc"):
                    raise ValueError("Native Linux dependencies differ from package metadata")
            elif host_dependencies != expected_dependencies:
                raise ValueError("Native macOS dependencies differ from package metadata")
            env = isolated_environment(root)
            process = subprocess.run([str(executable), "--build-info"], cwd=package, env=env, capture_output=True, text=True, timeout=15, check=True)
            if json.loads(process.stdout) != {"version": info["version"], "capture": False}:
                raise ValueError("The normal binary does not match its build metadata")
            if platform_name == "macos":
                subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(bundle)], check=True, timeout=30)
            report.update({"native_checked": True, "native_dependencies": host_dependencies, "scope": "Archive integrity, native system dependencies, permissions, build-info and macOS signature validity where applicable"})
            if smoke:
                if platform_name != "linux":
                    raise ValueError("Automated GUI smoke is Linux only; validate Finder/Gatekeeper/GUI manually on macOS")
                report["gui_smoke"] = smoke_linux(executable, package, documents, env, root)
                report["gui_checked"] = True
                report["scope"] += "; Linux GUI startup and sample JPEG decode under the supplied display"
    report["result"] = "passed"
    report["remaining_validation"] = "Clean desktop systems, hardware GPU/Wayland sessions, macOS Finder/Gatekeeper/GUI and accessibility"
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--report", type=Path)
    parser.add_argument("--smoke", action="store_true", help="Launch and decode the sample on Linux; supply DISPLAY or run under xvfb-run")
    parser.add_argument("--inspect-only", action="store_true", help="Inspect files without executing the binary or checking native dependencies")
    parser.add_argument("--expected-commit", help="Require this exact 40-character source SHA")
    args = parser.parse_args()
    result = verify(args.archive, args.smoke, args.inspect_only, args.expected_commit)
    rendered = json.dumps(result, indent=2) + "\n"
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(rendered, encoding="utf-8")
    print(rendered, end="")
