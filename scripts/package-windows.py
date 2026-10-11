"""Build a self-contained portable ZIP from an already compiled normal binary."""
import argparse
import hashlib
import json
import shutil
import subprocess
import tempfile
import tomllib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from third_party_notices import collect
from pe_imports import imports

REPO = Path(__file__).resolve().parent.parent


def package(executable, metadata, target, output, commit, compiler, rust_notices):
    version = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    info = json.loads(subprocess.check_output([str(executable.resolve()), "--build-info"], text=True))
    if info != {"version": version, "capture": False}:
        raise ValueError("The binary must match the workspace version and exclude capture")
    data = executable.read_bytes()
    dlls = imports(data)
    if not rust_notices.is_file():
        raise ValueError('Install rust-docs to include the standard library notices')
    if target not in {"x86_64-pc-windows-gnu", "x86_64-pc-windows-msvc"}:
        raise ValueError("Unsupported distribution target")
    output.mkdir(parents=True, exist_ok=True)
    name = f"rastera-{version}-windows-x64-{target.rsplit('-', 1)[1]}"
    archive = output / (name + ".zip")
    checksum = output / (name + ".zip.sha256")
    if archive.exists() or checksum.exists():
        raise FileExistsError("Use a new output directory; existing release assets are preserved")
    with tempfile.TemporaryDirectory(prefix="rastera-package-", dir=output) as temporary:
        stage = Path(temporary) / name
        stage.mkdir()
        shutil.copyfile(executable, stage / "rastera.exe")
        for file in ["LICENSE", "CHANGELOG.md", "SECURITY.md"]:
            shutil.copyfile(REPO / file, stage / file)
        shutil.copyfile(REPO / "docs/install-windows.md", stage / "LEEME.md")
        (stage / "docs").mkdir()
        shutil.copyfile(REPO / "docs/usage.md", stage / "docs/usage.md")
        shutil.copytree(REPO / "docs/tutorial", stage / "docs/tutorial")
        (stage / "assets/branding").mkdir(parents=True)
        for file in ["rastera.svg", "rastera-32.png"]:
            shutil.copyfile(REPO / "assets/branding" / file, stage / "assets/branding" / file)
        collect(metadata, stage)
        shutil.copyfile(rust_notices, stage / 'RUST-STANDARD-LIBRARY-NOTICES.html')
        if target.endswith('-gnu'):
            shutil.copyfile(REPO / 'licensing/toolchain/COPYING.MinGW-w64-runtime.txt', stage / 'MINGW-RUNTIME-NOTICES.txt')
        (stage / "BUILD-INFO.json").write_text(json.dumps({**info, "target": target, "source_commit": commit, "compiler": compiler, "executable_sha256": hashlib.sha256(data).hexdigest(), 'imported_system_dlls': dlls, 'rust_library_notices_sha256': hashlib.sha256(rust_notices.read_bytes()).hexdigest()}, indent=2) + "\n", encoding="utf-8")
        with ZipFile(archive, "x", ZIP_DEFLATED) as zip_file:
            for file in sorted(stage.rglob("*")):
                if file.is_file():
                    zip_file.write(file, str(file.relative_to(stage.parent)).replace("\\", "/"))
    checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n", encoding="ascii")
    print(archive)
    print(checksum)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for key in ["executable", "metadata", "output", "rust-notices"]:
        parser.add_argument("--" + key, type=Path, required=True)
    for key in ["target", "commit", "compiler"]:
        parser.add_argument("--" + key, required=True)
    args = parser.parse_args()
    package(args.executable, args.metadata, args.target, args.output, args.commit, args.compiler, args.rust_notices)
