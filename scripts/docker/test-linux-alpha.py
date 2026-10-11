"""Exercise a shipping Linux alpha in a disposable, non-root desktop profile.

This is an integration driver, not a Rust build. SQLite annotation and previous
installation seeds are explicitly fixtures; they do not prove UI editing or an
upgrade from a previously executed binary. Only the driver-owned app processes
are terminated. The supplied display server, D-Bus session and graphics stack remain
under the container entrypoint's control.
"""
import argparse
import hashlib
import importlib.util
import json
import os
import platform
import re
import shutil
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
import traceback
from pathlib import Path

VERSION = "0.1.0-alpha.3"
TARGET = "x86_64-unknown-linux-gnu"
SOURCE_COMMIT = "382b55a19221618d10af4a4cdf94e97062408f50"
APP_ID = "io.github.Alexrb11.rastera"
FIXTURE_TAG = "fixture-persistence"
REPOSITORY = Path(__file__).resolve().parents[2]


def digest(path):
    value = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def record(report, name, **details):
    report["checks"].append({"name": name, "status": "passed", "details": details})


def command(arguments, environment, log_path, timeout=60, should_fail=False):
    with log_path.open("w", encoding="utf-8") as log:
        result = subprocess.run(
            [str(value) for value in arguments], env=environment,
            stdout=log, stderr=subprocess.STDOUT, timeout=timeout,
        )
    if (result.returncode == 0) == should_fail:
        raise ValueError(
            "Unexpected exit code " + str(result.returncode) + ": "
            + log_path.read_text(encoding="utf-8", errors="replace")[-6000:]
        )
    return result.returncode


def display_backend(env):
    backend = env.get("RASTERA_DISPLAY_BACKEND", "x11")
    require(backend in {"x11", "wayland"}, "RASTERA_DISPLAY_BACKEND must be x11 or wayland")
    return backend


def validate_display(env):
    backend = display_backend(env)
    if backend == "x11":
        require(bool(env.get("DISPLAY")), "X11 GUI checks require the entrypoint's active X display")
        require(not env.get("WAYLAND_DISPLAY") and not env.get("WAYLAND_SOCKET"),
                "X11 checks must not expose a Wayland display or inherited connection")
        return None
    require("DISPLAY" not in env, "Native Wayland checks require DISPLAY to be unset; X11 fallback is forbidden")
    require(not env.get("WAYLAND_SOCKET"), "Wayland checks must connect to the named compositor socket")
    runtime = Path(env.get("XDG_RUNTIME_DIR", ""))
    require(runtime.is_absolute() and runtime.is_dir() and not runtime.is_symlink(),
            "Wayland checks require the compositor's absolute XDG_RUNTIME_DIR")
    require(runtime.stat().st_uid == os.geteuid() and stat.S_IMODE(runtime.stat().st_mode) == 0o700,
            "The compositor runtime directory must belong to this user and have mode 0700")
    name = env.get("WAYLAND_DISPLAY", "")
    require(bool(name), "Wayland checks require WAYLAND_DISPLAY")
    socket = Path(name) if Path(name).is_absolute() else runtime / name
    require(socket.is_socket(), "The supplied WAYLAND_DISPLAY is not a live Unix socket: " + str(socket))
    return socket


def wayland_protocol(log_path):
    """Require a configured Rastera toplevel with a committed non-null buffer.

    Both wayland-backend's Rust debug output and libwayland's client trace use
    interface@id.method (newer libwayland also uses interface#id.method).
    """
    quoted_id = re.escape(json.dumps(APP_ID))
    requests = {
        "display_registry": r"\bwl_display[@#]\d+\.get_registry\s*\(",
        "registry_globals": r"\bwl_registry[@#]\d+\.global\s*,?\s*\(",
    }
    lines = log_path.read_text(encoding="utf-8", errors="replace").splitlines()
    evidence = {name: next((line for line in lines if re.search(pattern, line)), None)
                for name, pattern in requests.items()}
    missing = [name for name, line in evidence.items() if line is None]
    require(not missing, "The Rastera client log lacks native Wayland protocol evidence: " + ", ".join(missing))

    def locate(pattern, start=0, stop=None):
        expression = re.compile(pattern)
        for index in range(start, len(lines) if stop is None else stop):
            match = expression.search(lines[index])
            if match:
                return index, match
        return None

    bindings = re.compile(r"\bxdg_wm_base[@#]\d+\.get_xdg_surface\s*\("
                          r"[^)]*?\bxdg_surface[@#](\d+)\b[^)]*?\bwl_surface[@#](\d+)\b")
    for binding_index, line in enumerate(lines):
        binding = bindings.search(line)
        if not binding:
            continue
        xdg_id, surface_id = binding.groups()
        surface = r"\bwl_surface[@#]" + surface_id + r"\."
        xdg = r"\bxdg_surface[@#]" + xdg_id + r"\."
        created = locate(r"\bwl_compositor[@#]\d+\.create_surface\s*\("
                         r"[^)]*?\bwl_surface[@#]" + surface_id + r"\b", stop=binding_index)
        toplevel = locate(xdg + r"get_toplevel\s*\([^)]*?\bxdg_toplevel[@#](\d+)\b",
                          binding_index + 1)
        if not created or not toplevel:
            continue
        toplevel_index, toplevel_match = toplevel
        toplevel_id = toplevel_match[1]
        application = locate(r"\bxdg_toplevel[@#]" + toplevel_id
                             + r"\.set_app_id\s*\(\s*(?:" + quoted_id
                             + r"|Some\(\s*" + quoted_id + r"\s*\))\s*\)", toplevel_index + 1)
        if not application:
            continue
        configure_pattern = re.compile(xdg + r"configure\s*,?\s*\(\s*(\d+)\s*,?\s*\)")
        # Compare trace order and protocol object IDs, never numeric timestamps.
        # A compositor may send several configure events before the client acks.
        for configure_index in range(toplevel_index + 1, len(lines)):
            configure = configure_pattern.search(lines[configure_index])
            if not configure:
                continue
            ack = locate(xdg + r"ack_configure\s*\(\s*" + configure[1] + r"\s*,?\s*\)",
                         configure_index + 1)
            if not ack:
                continue
            attach_pattern = re.compile(surface + r"attach\s*\(\s*(?:Some\(\s*)?wl_buffer[@#](\d+)\b")
            for attach_index in range(ack[0] + 1, len(lines)):
                attached = attach_pattern.search(lines[attach_index])
                if not attached or int(attached[1]) == 0:
                    continue
                # Ignore decoration subsurfaces and any buffer replaced/detached
                # before commit: only the Rastera toplevel's linked surface counts.
                action = locate(surface + r"(attach|commit)\s*\(", attach_index + 1)
                if not action or action[1][1] != "commit":
                    continue
                evidence.update({
                    "create_surface": lines[created[0]], "xdg_surface": line,
                    "xdg_toplevel": lines[toplevel_index], "application_id": lines[application[0]],
                    "surface_configure": lines[configure_index], "ack_configure": lines[ack[0]],
                    "attached_buffer": lines[attach_index], "surface_commit": lines[action[0]],
                })
                return {"log": log_path.name, "log_sha256": digest(log_path),
                        "observed_messages": evidence, "application_id": APP_ID,
                        "surface_ids": {"wl_surface": int(surface_id), "xdg_surface": int(xdg_id),
                                        "xdg_toplevel": int(toplevel_id), "wl_buffer": int(attached[1])},
                        "buffer_committed": True, "x11_fallback_available": False}
    raise ValueError("The Rastera client log lacks a linked native Wayland toplevel with the expected "
                     "application ID, configure/ack and non-null buffer attachment followed by commit")


def profile(root):
    """Use a HOME containing spaces to exercise installed launcher quoting."""
    env = dict(os.environ)
    for key in list(env):
        if key.startswith("DYLD_") or key in {"LD_PRELOAD", "LD_LIBRARY_PATH", "LD_AUDIT"}:
            env.pop(key)
    home = root / "home with spaces"
    paths = {
        "HOME": home,
        "XDG_DATA_HOME": home / ".local/share",
        "XDG_CACHE_HOME": home / ".cache",
        "XDG_CONFIG_HOME": home / ".config",
    }
    if display_backend(env) == "wayland":
        # The compositor's socket is relative to this runtime directory. HOME,
        # catalog, cache and config remain disposable; the socket must be shared.
        validate_display(env)
    else:
        paths["XDG_RUNTIME_DIR"] = root / "runtime"
    for key, path in paths.items():
        path.mkdir(parents=True, mode=0o700)
        path.chmod(0o700)
        env[key] = str(path)
    return env


def catalog_path(env):
    return Path(env["XDG_DATA_HOME"]) / "Rastera/catalog.sqlite"


def read_image(catalog, sample):
    if not catalog.is_file():
        return None
    with sqlite3.connect(catalog.resolve().as_uri() + "?mode=ro", uri=True) as database:
        row = database.execute(
            "SELECT favorite, tags, width, height, format FROM images WHERE path = ?",
            (os.fsencode(sample.resolve()),),
        ).fetchone()
    if row is None:
        return None
    return dict(zip(("favorite", "tags", "width", "height", "format"), row))


def gui_run(executable, pictures, env, log_path, timeout):
    socket = validate_display(env)
    backend = display_backend(env)
    client_env = dict(env)
    if backend == "wayland":
        client_env["WAYLAND_DEBUG"] = "client"
    else:
        client_env.pop("WAYLAND_DEBUG", None)
    sample = sorted(pictures.glob("*.jpg"))[0]
    catalog = catalog_path(env)
    with log_path.open("w", encoding="utf-8") as log:
        process = subprocess.Popen(
            [str(executable), str(pictures)], env=client_env, cwd=pictures,
            stdout=log, stderr=subprocess.STDOUT,
        )
        try:
            deadline = time.monotonic() + timeout
            while time.monotonic() < deadline:
                if process.poll() is not None:
                    raise ValueError("The GUI exited before decoding its selected image: "
                                     + log_path.read_text(errors="replace")[-6000:])
                try:
                    image = read_image(catalog, sample)
                except sqlite3.Error:
                    image = None  # Creation/migration may still be in progress.
                if image and image["width"] and image["height"] and image["format"]:
                    time.sleep(2)
                    require(process.poll() is None, "The GUI exited immediately after sample decode")
                    break
                time.sleep(0.2)
            else:
                raise ValueError("No decoded metadata for the selected copied JPEG within " + str(timeout) + " seconds")
        finally:
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=5)
    detail = {"selected_sample": sample.name, "decoded_image": image,
              "catalog_created": True, "display_backend": backend,
              "close_method": "driver terminates its own process; graceful close untested"}
    if backend == "wayland":
        detail["wayland_socket"] = str(socket)
        detail["wayland_protocol"] = wayland_protocol(log_path)
    return detail


def seed_annotation(catalog, sample):
    """Run only after gui_run has waited for the app process to exit."""
    with sqlite3.connect(catalog) as database:
        result = database.execute("UPDATE images SET favorite = 1, tags = ? WHERE path = ?",
                                  (FIXTURE_TAG, os.fsencode(sample.resolve())))
        require(result.rowcount == 1, "The persistence fixture needs one genuinely decoded image")
        database.commit()
        database.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchall()
    return read_image(catalog, sample)


def assert_annotation(catalog, sample, expected):
    image = read_image(catalog, sample)
    require(image == expected, "The installed application changed or lost the annotation fixture: " + repr(image))


def clear_decode_fixture(catalog, sample):
    """Require a fresh decode at each relaunch, preserving the annotation seed."""
    with sqlite3.connect(catalog) as database:
        result = database.execute(
            "UPDATE images SET width = NULL, height = NULL, format = NULL WHERE path = ?",
            (os.fsencode(sample.resolve()),),
        )
        require(result.rowcount == 1, "Missing previously decoded persistence fixture")
        database.commit()
        database.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchall()


def mime_fixture(env):
    path = Path(env["XDG_CONFIG_HOME"]) / "mimeapps.list"
    path.write_text("[Default Applications]\nimage/jpeg=existing-fixture-viewer.desktop;\n"
                    "image/png=existing-fixture-viewer.desktop;\n"
                    "image/webp=existing-fixture-viewer.desktop;\n", encoding="utf-8")
    return path, path.read_bytes()


def installed_paths(env):
    local = Path(env["HOME"]) / ".local"
    return {
        "release": local / "share/rastera/releases" / VERSION,
        "launcher": local / "bin/rastera",
        "desktop": local / ("share/applications/" + APP_ID + ".desktop"),
        "icon": local / ("share/icons/hicolor/256x256/apps/" + APP_ID + ".png"),
        "ownership": local / "share/rastera/installation.json",
    }


def check_install(env, package, info, registered, log_path):
    paths = installed_paths(env)
    executable = paths["release"] / "bin/rastera"
    for key in ("launcher", "desktop", "icon", "ownership"):
        require(paths[key].is_file(), "Missing installed " + key)
    require(paths["launcher"].stat().st_mode & stat.S_IXUSR, "The installed launcher is not executable")
    require(digest(executable) == info["executable_sha256"], "The installer changed the release binary")
    require(digest(paths["icon"]) == digest(package / "assets/branding/rastera-256.png"),
            "The installed icon differs from the package")
    entry = paths["desktop"].read_text(encoding="utf-8")
    require(('MimeType=image/jpeg;image/png;image/webp;' in entry) == registered,
            "MIME associations must be opt-in")
    require('Exec="' in entry and str(executable) in entry and " %f" in entry,
            "The desktop entry must quote its absolute installation path")
    require(json.loads(paths["ownership"].read_text())["version"] == VERSION,
            "The installation ownership marker has the wrong version")
    command([paths["launcher"], "--build-info"], env, log_path, timeout=15)
    require(json.loads(log_path.read_text()) == {"version": VERSION, "capture": False},
            "The installed launcher did not execute the shipping binary")
    return paths


def previous_installation_fixture(env):
    paths = installed_paths(env)
    prior = paths["release"].parent / "0.1.0-alpha.2"
    prior.mkdir(parents=True)
    retained = prior / "fixture.txt"
    retained.write_text("Installer ownership fixture only; no alpha.2 binary was executed.\n")
    for key in ("launcher", "desktop", "icon"):
        paths[key].parent.mkdir(parents=True, exist_ok=True)
        paths[key].write_text("Owned previous installation fixture\n")
    paths["ownership"].write_text(json.dumps({
        "version": "0.1.0-alpha.2",
        "created_files": [str(paths[key]) for key in ("launcher", "desktop", "icon")],
    }))
    return retained, retained.read_bytes()


def exercise(args, report):
    require(platform.system() == "Linux", "Run this driver inside a Linux container")
    require(os.geteuid() != 0, "Run as the unprivileged container tester, not root")
    require(not shutil.which("cargo") and not shutil.which("rustc"),
            "The runtime container must not include Rust/Cargo on PATH")
    backend = display_backend(os.environ)
    socket = validate_display(os.environ) if args.gui else None
    report["environment"] = {
        "platform": platform.platform(), "machine": platform.machine(),
        "python": platform.python_version(), "uid": os.geteuid(),
        "os_release": Path("/etc/os-release").read_text() if Path("/etc/os-release").is_file() else None,
        "rust_on_path": False, "display": os.environ.get("DISPLAY"),
        "display_backend": backend, "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        "wayland_socket": str(socket) if socket else None,
        "dbus_session_available": bool(os.environ.get("DBUS_SESSION_BUS_ADDRESS")),
    }
    record(report, "runtime_container", non_root=True, rust_required=False)
    verifier = REPOSITORY / "scripts/verify-unix-package.py"
    native_report = args.report_dir / "native-verification.json"
    command([sys.executable, verifier, "--archive", args.archive,
             "--expected-commit", args.expected_commit, "--report", native_report],
            os.environ.copy(), args.report_dir / "native-verification.log", timeout=180)
    native = json.loads(native_report.read_text())
    require(native["result"] == "passed" and native["native_checked"], "Native package verification did not complete")
    require(native["version"] == VERSION and native["target"] == TARGET, "Expected the Linux x64 alpha.3 release")
    report.update({"source_commit": native["source_commit"], "archive_sha256": native["archive_sha256"]})
    record(report, "native_release_verification", verification_report="native-verification.json",
           scope=native["scope"])
    sys.path.insert(0, str(verifier.parent))
    spec = importlib.util.spec_from_file_location("unix_release_verifier", verifier)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with tempfile.TemporaryDirectory(prefix="rastera-container-test-") as temporary:
        root = Path(temporary)
        extracted = root / "extracted"
        extracted.mkdir()
        package = module.safe_extract(args.archive, extracted)
        info = json.loads((package / "BUILD-INFO.json").read_text())
        record(report, "safe_extraction", archive_permissions="validated by shared release verifier")
        env = profile(root / "current-profile")
        pictures = root / "Pictures" / "Rastera practice copies"
        pictures.mkdir(parents=True)
        originals = sorted((package / "docs/tutorial/copias").glob("*.jpg"))
        require(bool(originals), "The release must contain tutorial JPEGs")
        # The first streamed folder page follows filesystem enumeration order.
        # A single-image folder gives this non-interactive driver a deterministic
        # selected preview without assuming a sort order or clicking coordinates.
        for original in originals[:1]:
            shutil.copyfile(original, pictures / original.name)
        sample = sorted(pictures.glob("*.jpg"))[0]
        photo_hashes = {path.name: digest(path) for path in pictures.iterdir()}
        annotations = None
        if args.gui:
            detail = gui_run(package / "bin/rastera", pictures, env,
                             args.report_dir / "portable-gui.log", args.timeout)
            record(report, "portable_gui_folder_decode", **detail)
            annotations = seed_annotation(catalog_path(env), sample)
            record(report, "annotation_persistence_fixture", seed=annotations,
                   scope="SQLite fixture written only after the application exited; UI editing is untested")
        mime, default_bytes = mime_fixture(env)
        catalog_hash = digest(catalog_path(env)) if annotations else None
        command(["/bin/sh", package / "install-user.sh"], env,
                args.report_dir / "install-user.log")
        installed = check_install(env, package, info, False, args.report_dir / "installed-build-info.log")
        require(mime.read_bytes() == default_bytes, "The installer changed default image handlers")
        if annotations:
            require(digest(catalog_path(env)) == catalog_hash, "Installation modified the existing catalog file")
            assert_annotation(catalog_path(env), sample, annotations)
        record(report, "per_user_installation", home_path_contains_spaces=True,
               launcher_build_info_checked=True, default_handlers_preserved=True,
               existing_catalog_unchanged=bool(annotations))
        install_hashes = {key: digest(installed[key]) for key in ("launcher", "desktop", "icon", "ownership")}
        code = command(["/bin/sh", package / "install-user.sh"], env,
                       args.report_dir / "reinstall-refusal.log", should_fail=True)
        require(install_hashes == {key: digest(installed[key]) for key in install_hashes},
                "Rejected same-version installation modified integration files")
        require(mime.read_bytes() == default_bytes, "Rejected reinstallation changed default handlers")
        if annotations:
            require(digest(catalog_path(env)) == catalog_hash, "Rejected reinstallation modified the catalog")
        record(report, "same_version_reinstallation_refused", exit_code=code,
               integration_files_unchanged=True)
        if args.gui:
            clear_decode_fixture(catalog_path(env), sample)
            detail = gui_run(installed["launcher"], pictures, env,
                             args.report_dir / "installed-gui.log", args.timeout)
            assert_annotation(catalog_path(env), sample, annotations)
            record(report, "installed_gui_annotation_persistence",
                   decode_fixture="Cleared only dimensions/format with app closed to require fresh decode",
                   **detail)
        upgraded = profile(root / "replacement-profile")
        retained, retained_bytes = previous_installation_fixture(upgraded)
        upgrade_mime, upgrade_defaults = mime_fixture(upgraded)
        if annotations:
            # No app is running; checkpoint before copying a genuine catalog.
            with sqlite3.connect(catalog_path(env)) as database:
                database.execute("PRAGMA wal_checkpoint(TRUNCATE)").fetchall()
            catalog_path(upgraded).parent.mkdir(parents=True)
            shutil.copyfile(catalog_path(env), catalog_path(upgraded))
            upgrade_catalog_hash = digest(catalog_path(upgraded))
        command(["/bin/sh", package / "install-user.sh", "--replace", "--register-mime"],
                upgraded, args.report_dir / "replacement-install.log")
        replaced = check_install(upgraded, package, info, True,
                                 args.report_dir / "replacement-build-info.log")
        require(retained.read_bytes() == retained_bytes, "Replacing launchers deleted the previous release fixture")
        require(upgrade_mime.read_bytes() == upgrade_defaults, "Opt-in associations changed default image handlers")
        if annotations:
            require(digest(catalog_path(upgraded)) == upgrade_catalog_hash, "Replacement modified the catalog fixture")
            assert_annotation(catalog_path(upgraded), sample, annotations)
        record(report, "owned_installation_replacement_fixture",
               scope="Previous ownership marker/files are fixtures; no older Rastera binary was executed",
               previous_release_retained=True, mime_associations_opt_in=True,
               default_handlers_preserved=True, catalog_fixture_unchanged=bool(annotations))
        if args.gui:
            clear_decode_fixture(catalog_path(upgraded), sample)
            detail = gui_run(replaced["launcher"], pictures, upgraded,
                             args.report_dir / "upgrade-gui.log", args.timeout)
            assert_annotation(catalog_path(upgraded), sample, annotations)
            record(report, "replacement_gui_annotation_persistence",
                   decode_fixture="Cleared only dimensions/format with app closed to require fresh decode",
                   **detail)
        else:
            report["checks"].append({"name": "gui_and_catalog_persistence", "status": "skipped",
                                     "details": {"reason": "--gui was not requested"}})
        require(photo_hashes == {path.name: digest(path) for path in pictures.iterdir()},
                "Launching or installing modified the copied sample photographs")
        record(report, "sample_copies_preserved", count=len(photo_hashes), sha256=photo_hashes,
               originals_scope="Only package-provided examples were copied; user photographs are never accessed")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--report-dir", type=Path, required=True)
    parser.add_argument("--gui", action="store_true")
    parser.add_argument("--expected-commit", default=SOURCE_COMMIT)
    parser.add_argument("--timeout", type=int, default=60, help="Seconds allowed for each GUI decode (5-180)")
    args = parser.parse_args()
    if not re.fullmatch(r"[a-f0-9]{40}", args.expected_commit) or not 5 <= args.timeout <= 180:
        parser.error("Use a full lowercase source SHA and a timeout between 5 and 180 seconds")
    args.archive = args.archive.resolve()
    args.report_dir = args.report_dir.resolve()
    args.report_dir.mkdir(parents=True, exist_ok=True)
    report = {
        "result": "failed", "archive": args.archive.name, "version": VERSION, "target": TARGET,
        "expected_source_commit": args.expected_commit, "gui_requested": args.gui, "checks": [],
        "remaining_validation": [
            "UI favorite/tag editing, folder watcher changes and batch tools require separate interaction tests",
            "Real desktop compositors and hardware GPU, file dialogs, accessibility and graceful app close",
            "A real older binary upgrade; this driver uses a declared previous-installation fixture",
            "Docker shares the host kernel; this does not validate a different Linux kernel or macOS",
        ],
    }
    try:
        exercise(args, report)
        report["result"] = "passed"
    except Exception as error:
        report["error"] = str(error)
        (args.report_dir / "failure.log").write_text(traceback.format_exc(), encoding="utf-8")
    (args.report_dir / "results.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report, indent=2))
    return 0 if report["result"] == "passed" else 1


if __name__ == "__main__":
    sys.exit(main())
