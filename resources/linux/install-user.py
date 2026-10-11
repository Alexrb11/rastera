"""Opt-in per-user Linux installation; never changes default MIME handlers."""
import argparse
import json
import re
import shlex
import shutil
import subprocess
from pathlib import Path

APP_ID = "io.github.Alexrb11.rastera"


def desktop_argument(value):
    # Desktop Exec is not a shell command. Escape both the string-value layer
    # and the reserved characters inside a quoted Exec argument.
    if any(c in value for c in "\n\r\0"):
        raise ValueError("Unsupported installation path")
    value = value.replace("\\", "\\\\\\\\")
    for character in ('"', '`', '$'):
        value = value.replace(character, "\\\\" + character)
    return '"' + value.replace("%", "%%") + '"'


def install(register_mime=False, replace=False):
    source = Path(__file__).resolve().parent
    info = json.loads((source / "BUILD-INFO.json").read_text())
    if info["target"] != "x86_64-unknown-linux-gnu" or info["capture"] is not False or not re.fullmatch(r"\d+\.\d+\.\d+(?:-alpha\.\d+)?", info.get("version", "")):
        raise ValueError("Expected a normal Linux release")
    home = Path.home()
    local = home / ".local"
    destination = local / "share/rastera/releases" / info["version"]
    launcher = local / "bin/rastera"
    desktop = local / ("share/applications/" + APP_ID + ".desktop")
    icon = local / ("share/icons/hicolor/256x256/apps/" + APP_ID + ".png")
    if destination.exists():
        raise FileExistsError("This version is already installed at " + str(destination))
    owned = local / "share/rastera/installation.json"
    previous = None
    if owned.is_file():
        previous = json.loads(owned.read_text())
    for path in (launcher, desktop, icon):
        if path.exists() or path.is_symlink():
            if path.is_symlink() or not path.is_file() or not replace or not previous or str(path) not in previous.get("created_files", []):
                raise FileExistsError("Existing file preserved: " + str(path) + "; use --replace for a previous Rastera installation")
    executable = str(destination / "bin/rastera")
    entry = (source / "share/applications/rastera.desktop").read_text()
    entry = entry.replace("Exec=rastera %f", "Exec=" + desktop_argument(executable) + " %f")
    if register_mime:
        entry += "MimeType=image/jpeg;image/png;image/webp;\n"
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)
    for path in (launcher, desktop, icon):
        path.parent.mkdir(parents=True, exist_ok=True)
    launcher.write_text("#!/bin/sh\nexec " + shlex.quote(executable) + ' "$@"\n')
    launcher.chmod(0o755)
    desktop.write_text(entry)
    shutil.copyfile(source / "assets/branding/rastera-256.png", icon)
    owned.write_text(json.dumps({"version": info["version"], "created_files": [str(path) for path in (launcher, desktop, icon)]}, indent=2) + "\n")
    updater = shutil.which("update-desktop-database")
    if updater:
        subprocess.run([updater, str(desktop.parent)], check=True)
    print("Installed in " + str(destination))
    print("Launcher: " + str(launcher) + "; add ~/.local/bin to PATH if needed")
    print("Default image applications were preserved.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--register-mime", action="store_true", help="Offer Rastera in Open With for JPEG/PNG/WebP; preserve default handlers")
    parser.add_argument("--replace", action="store_true", help="Update launchers from a previous Rastera installation; retain previous version folders")
    args = parser.parse_args()
    install(args.register_mime, args.replace)
