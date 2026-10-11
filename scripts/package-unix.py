"""Build and package a normal Linux or macOS release on a matching native host."""
import argparse
import json
import os
import plistlib
import shutil
import subprocess
import tarfile
import tempfile
import tomllib
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

from package_unix_support import TARGETS, architecture, bundle_plist, dependencies, native_target, sha256
from third_party_notices import collect

REPO = Path(__file__).resolve().parent.parent


def run(command, **kwargs):
    subprocess.run(command, check=True, cwd=REPO, **kwargs)


def common_files(stage, platform_name, metadata, rust_notices):
    stage.mkdir(parents=True, exist_ok=True)
    for name in ("LICENSE", "CHANGELOG.md", "SECURITY.md"):
        shutil.copyfile(REPO / name, stage / name)
    guide = (REPO / f"docs/install-{platform_name}.md").read_text(encoding="utf-8")
    guide = guide.replace("](usage.md)", "](docs/usage.md)").replace("](../SECURITY.md)", "](SECURITY.md)")
    guide = guide.replace("](releasing.md)", "](https://github.com/Alexrb11/rastera/blob/main/docs/releasing.md)")
    (stage / "README.md").write_text(guide, encoding="utf-8")
    (stage / "docs").mkdir()
    shutil.copyfile(REPO / "docs/usage.md", stage / "docs/usage.md")
    shutil.copytree(REPO / "docs/tutorial", stage / "docs/tutorial")
    (stage / "assets/branding").mkdir(parents=True)
    for name in ("rastera.svg", "rastera-32.png", "rastera-256.png"):
        shutil.copyfile(REPO / "assets/branding" / name, stage / "assets/branding" / name)
    collect(metadata, stage)
    shutil.copyfile(rust_notices, stage / "RUST-STANDARD-LIBRARY-NOTICES.html")


def make_icon(resources):
    # Apple iconutil accepts the conventional ten named PNG representations.
    with tempfile.TemporaryDirectory(prefix="rastera-icon-") as folder:
        iconset = Path(folder) / "rastera.iconset"
        iconset.mkdir()
        for logical in (16, 32, 128, 256, 512):
            for scale in (1, 2):
                name = f"icon_{logical}x{logical}" + ("@2x" if scale == 2 else "") + ".png"
                shutil.copyfile(REPO / f"assets/branding/rastera-{logical * scale}.png", iconset / name)
        run(["/usr/bin/iconutil", "--convert", "icns", "--output", str(resources / "rastera.icns"), str(iconset)])


def regular_modes(stage, executable_files):
    # Normalize permissions instead of inheriting developer umask or checkout bits.
    stage.chmod(0o755)
    for path in stage.rglob("*"):
        if path.is_symlink():
            raise ValueError("Release contents must not contain symlinks")
        path.chmod(0o755 if path.is_dir() or path in executable_files else 0o644)


def write_archive(stage, archive):
    if archive.name.endswith(".tar.gz"):
        def normalized(member):
            member.uid = member.gid = 0
            member.uname = member.gname = ""
            return member
        with tarfile.open(archive, "x:gz", format=tarfile.PAX_FORMAT) as stream:
            stream.add(stage, arcname=stage.name, filter=normalized)
    else:
        # zipfile stores Unix permission bits; the verifier explicitly restores them.
        with ZipFile(archive, "x", ZIP_DEFLATED) as stream:
            for path in sorted(stage.rglob("*")):
                if path.is_file():
                    stream.write(path, path.relative_to(stage.parent).as_posix())


def package(target, output):
    native_target(target)
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=REPO, text=True).strip():
        raise ValueError("Commit the complete release source before packaging; the checkout must be clean")
    version = tomllib.loads((REPO / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]["version"]
    platform_name, arch = TARGETS[target]
    name = f"rastera-{version}-{platform_name}-{arch}"
    output = output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    archive = output / (name + (".tar.gz" if platform_name == "linux" else ".zip"))
    checksum = Path(str(archive) + ".sha256")
    if archive.exists() or checksum.exists():
        raise FileExistsError("Use a new output directory; existing release assets are preserved")
    compiler = subprocess.check_output(["rustc", "--version"], cwd=REPO, text=True).strip()
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=REPO, text=True).strip()
    sysroot = Path(subprocess.check_output(["rustc", "--print", "sysroot"], cwd=REPO, text=True).strip())
    rust_notices = sysroot / "share/doc/rust/COPYRIGHT-library.html"
    if not rust_notices.is_file():
        raise ValueError("Install rust-docs with rustup component add rust-docs; COPYRIGHT-library.html is required")
    env = dict(os.environ)
    if platform_name == "macos":
        env["MACOSX_DEPLOYMENT_TARGET"] = "13.0"
    run(["cargo", "build", "--release", "--locked", "-p", "rastera-desktop", "--target", target], env=env)
    # Cargo configuration can override its target-dir; use metadata as authority.
    with tempfile.TemporaryDirectory(prefix="rastera-package-", dir=output) as temporary:
        metadata = Path(temporary) / "cargo-metadata.json"
        with metadata.open("w", encoding="utf-8") as stream:
            run(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", target], env=env, stdout=stream)
        target_directory = Path(json.loads(metadata.read_text())["target_directory"])
        executable = target_directory / target / "release/rastera"
        info = json.loads(subprocess.check_output([str(executable), "--build-info"], text=True, timeout=15))
        if info != {"version": version, "capture": False}:
            raise ValueError("The binary must match the workspace version and exclude capture")
        arch_description = architecture(executable, target)
        host_dependencies = dependencies(executable, target)
        stage = Path(temporary) / name
        stage.mkdir()
        executable_files = set()
        if platform_name == "linux":
            documents = stage
            binary = stage / "bin/rastera"
            binary.parent.mkdir()
            shutil.copyfile(executable, binary)
            common_files(documents, platform_name, metadata, rust_notices)
            (stage / "share/applications").mkdir(parents=True)
            shutil.copyfile(REPO / "resources/linux/rastera.desktop", stage / "share/applications/rastera.desktop")
            for file in ("install-user.sh", "install-user.py"):
                shutil.copyfile(REPO / "resources/linux" / file, stage / file)
            executable_files.update((binary, stage / "install-user.sh"))
        else:
            bundle = stage / "Rastera.app"
            documents = bundle / "Contents/Resources"
            binary = bundle / "Contents/MacOS/rastera"
            binary.parent.mkdir(parents=True)
            shutil.copyfile(executable, binary)
            common_files(documents, platform_name, metadata, rust_notices)
            with (bundle / "Contents/Info.plist").open("wb") as stream:
                plistlib.dump(bundle_plist(version), stream, sort_keys=True)
            make_icon(documents)
            executable_files.add(binary)
        regular_modes(stage, executable_files)
        if platform_name == "macos":
            run(["/usr/bin/codesign", "--force", "--sign", "-", "--timestamp=none", str(bundle)])
            run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(bundle)])
        # BUILD-INFO is outside the signed bundle: its final executable digest
        # cannot be part of the resource seal which itself changes that binary.
        build_info = {
            **info, "target": target, "source_commit": commit, "compiler": compiler,
            "executable": binary.relative_to(stage).as_posix(),
            "documentation_root": documents.relative_to(stage).as_posix() if documents != stage else ".",
            "executable_sha256": sha256(binary),
            "rust_library_notices_sha256": sha256(rust_notices),
            "architecture": arch_description, "dependencies": host_dependencies,
            "signing": "ad-hoc; no Developer ID or notarization" if platform_name == "macos" else "unsigned",
            "validation_scope": "Native dependency and build-info checks; GUI and clean systems need separate validation",
        }
        (stage / "BUILD-INFO.json").write_text(json.dumps(build_info, indent=2) + "\n", encoding="utf-8")
        (stage / "BUILD-INFO.json").chmod(0o644)
        write_archive(stage, archive)
    checksum.write_text(sha256(archive) + "  " + archive.name + "\n", encoding="ascii")
    print(archive)
    print(checksum)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--output", type=Path, default=REPO / "work/releases")
    args = parser.parse_args()
    package(args.target, args.output)
