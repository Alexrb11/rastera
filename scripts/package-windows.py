"""Build a self-contained portable ZIP from an already compiled normal binary."""
import argparse
import hashlib
import json
import shutil
import struct
import subprocess
import tempfile
import tomllib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from third_party_notices import collect

REPO = Path(__file__).resolve().parent.parent


def package(executable, metadata, target, output, commit, compiler):
    version = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    info = json.loads(subprocess.check_output([str(executable.resolve()), "--build-info"], text=True))
    if info != {"version": version, "capture": False}:
        raise ValueError("The binary must match the workspace version and exclude capture")
    data = executable.read_bytes()
    pe = struct.unpack_from("<I", data, 0x3C)[0]
    if data[:2] != b"MZ" or data[pe:pe + 4] != b"PE\0\0" or struct.unpack_from("<H", data, pe + 4)[0] != 0x8664:
        raise ValueError("Expected a Windows x64 executable")
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
        collect(metadata, stage)
        (stage / "BUILD-INFO.json").write_text(json.dumps({**info, "target": target, "source_commit": commit, "compiler": compiler, "executable_sha256": hashlib.sha256(data).hexdigest()}, indent=2) + "\n", encoding="utf-8")
        with ZipFile(archive, "x", ZIP_DEFLATED) as zip_file:
            for file in sorted(stage.rglob("*")):
                if file.is_file():
                    zip_file.write(file, str(file.relative_to(stage.parent)).replace("\\", "/"))
    checksum.write_text(hashlib.sha256(archive.read_bytes()).hexdigest() + "  " + archive.name + "\n", encoding="ascii")
    print(archive)
    print(checksum)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    for key in ["executable", "metadata", "output"]:
        parser.add_argument("--" + key, type=Path, required=True)
    for key in ["target", "commit", "compiler"]:
        parser.add_argument("--" + key, required=True)
    args = parser.parse_args()
    package(args.executable, args.metadata, args.target, args.output, args.commit, args.compiler)
