"""Native Unix package checks shared by the packager and verifier."""
import hashlib
import os
import platform
import plistlib
import re
import struct
import subprocess
from pathlib import Path, PurePosixPath

TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x64"),
    "x86_64-apple-darwin": ("macos", "x64"),
    "aarch64-apple-darwin": ("macos", "arm64"),
}
BUNDLE_ID = "io.github.Alexrb11.rastera"
MACOS_MINIMUM = "13.0"


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def relative_path(value):
    """Reject traversal, alternate separators and ambiguous archive names."""
    if not isinstance(value, str) or not value or "\\" in value or "\0" in value:
        raise ValueError("Invalid package path")
    path = PurePosixPath(value)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in value.split("/")):
        raise ValueError("Unsafe package path: " + value)
    if any(":" in part for part in path.parts):
        raise ValueError("Drive names are not allowed in packages")
    return path


def native_target(target):
    if target not in TARGETS:
        raise ValueError("Unsupported distribution target")
    system, arch = TARGETS[target]
    current_system = {"Linux": "linux", "Darwin": "macos"}.get(platform.system())
    current_arch = {"x86_64": "x64", "amd64": "x64", "aarch64": "arm64", "arm64": "arm64"}.get(platform.machine().lower())
    if (current_system, current_arch) != (system, arch):
        raise ValueError("Build and execute packages on a matching native host")


def architecture(executable, target):
    with Path(executable).open("rb") as stream:
        data = stream.read(32)
    if target == "x86_64-unknown-linux-gnu":
        if len(data) < 20 or data[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", data, 18)[0] != 62:
            raise ValueError("Expected a little-endian ELF64 x86_64 executable")
        kind = struct.unpack_from("<H", data, 16)[0]
        if kind not in {2, 3}:  # ET_EXEC or position-independent ET_DYN
            raise ValueError("Expected an ELF executable")
        return "ELF64 x86_64"
    expected = {"x86_64-apple-darwin": 0x01000007, "aarch64-apple-darwin": 0x0100000C}.get(target)
    if len(data) < 16 or expected is None or struct.unpack_from("<II", data) != (0xFEEDFACF, expected):
        raise ValueError("Expected a thin Mach-O executable for the selected target")
    if struct.unpack_from("<I", data, 12)[0] != 2:  # MH_EXECUTE
        raise ValueError("Expected a Mach-O executable")
    return "Mach-O " + TARGETS[target][1]


def output(command, **kwargs):
    return subprocess.check_output(command, text=True, timeout=30, **kwargs)


def dependencies(executable, target):
    """Inspect dependencies on the native host; do not bundle third-party dylibs."""
    native_target(target)
    architecture(executable, target)
    if TARGETS[target][0] == "macos":
        text = output(["/usr/bin/otool", "-L", str(executable)])
        imports = [line.strip().split(" (", 1)[0] for line in text.splitlines()[1:] if line.strip()]
        if not imports or any(not name.startswith(("/usr/lib/", "/System/Library/")) for name in imports):
            raise ValueError("macOS packages may depend only on system dylibs/frameworks: " + repr(imports))
        load_commands = output(["/usr/bin/otool", "-l", str(executable)])
        minimums = []
        for block in re.split(r"Load command \d+", load_commands):
            if re.search(r"\bcmd LC_BUILD_VERSION\b", block):
                match = re.search(r"\bminos (\d+(?:\.\d+)+)", block)
            elif re.search(r"\bcmd LC_VERSION_MIN_MACOSX\b", block):
                match = re.search(r"\bversion (\d+(?:\.\d+)+)", block)
            else:
                continue
            if match:
                minimums.append(tuple(map(int, match[1].split("."))))
        if not minimums or any(value[:2] > (13, 0) for value in minimums):
            raise ValueError("The Mach-O deployment target exceeds macOS 13.0")
        return {"system_libraries": imports, "minimum_macos": MACOS_MINIMUM}
    env = dict(os.environ)
    for key in ("LD_LIBRARY_PATH", "LD_PRELOAD", "LD_AUDIT"):
        env.pop(key, None)
    env["LC_ALL"] = "C"
    text = output(["ldd", str(executable)], env=env)
    if "not found" in text or "not a dynamic executable" in text:
        raise ValueError("Linux shared library dependencies are unresolved")
    libraries = []
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("linux-vdso"):
            continue
        value = line.split("=>", 1)[-1].strip().split(" (", 1)[0]
        if not value.startswith(("/lib/", "/lib64/", "/usr/lib/", "/usr/lib64/")):
            raise ValueError("Linux dependency is outside system library paths: " + value)
        if not Path(value).is_file():
            raise ValueError("Unresolved Linux dependency: " + value)
        libraries.append({"name": Path(value).name, "path": value})
    if not libraries:
        raise ValueError("Expected system shared libraries")
    versions = output(["readelf", "--version-info", str(executable)], env=env)
    required = sorted({tuple(map(int, value.split("."))) for value in re.findall(r"GLIBC_(\d+(?:\.\d+)+)", versions)})
    if not required or required[-1] > (2, 35):
        raise ValueError("Linux package exceeds the Ubuntu 22.04 / GLIBC 2.35 baseline")
    return {"system_libraries": libraries, "maximum_required_glibc": ".".join(map(str, required[-1])), "glibc_baseline": "2.35"}


def bundle_plist(version):
    match = re.fullmatch(r"(\d+\.\d+\.\d+)(?:-alpha\.(\d+))?", version)
    if not match:
        raise ValueError("Unsupported bundle version")
    base = match[1]
    return {
        "CFBundleDevelopmentRegion": "en",
        "CFBundleDisplayName": "Rastera",
        "CFBundleName": "Rastera",
        "CFBundleIdentifier": BUNDLE_ID,
        "CFBundleExecutable": "rastera",
        "CFBundlePackageType": "APPL",
        "CFBundleInfoDictionaryVersion": "6.0",
        "CFBundleShortVersionString": base,
        "CFBundleVersion": base,
        "CFBundleIconFile": "rastera.icns",
        "LSMinimumSystemVersion": MACOS_MINIMUM,
        "NSHighResolutionCapable": True,
        "RasteraVersion": version,
        # File arguments work from the CLI. Finder document associations require
        # handling Apple events, so this bundle intentionally declares none.
    }


def check_bundle(package, version):
    bundle = package / "Rastera.app"
    with (bundle / "Contents/Info.plist").open("rb") as stream:
        info = plistlib.load(stream)
    if info != bundle_plist(version):
        raise ValueError("The application bundle metadata does not match the release")
    if not (bundle / "Contents/Resources/rastera.icns").is_file():
        raise ValueError("Missing macOS application icon")
    return bundle
