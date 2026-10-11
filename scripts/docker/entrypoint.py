"""Run native Linux checks or an interactive X11/Wayland desktop with the release."""
import argparse
import contextlib
import datetime
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import stat
import subprocess
import sys
import tarfile
import tempfile
import time


SCRIPTS = Path("/workspace/scripts")
REPORTS = Path("/reports")
WAYLAND_SOCKET = "rastera-wayland"


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def archive_metadata(archive):
    with tarfile.open(archive, "r:gz") as stream:
        matches = [member for member in stream.getmembers()
                   if member.name.endswith("/BUILD-INFO.json")]
        if len(matches) != 1 or not matches[0].isreg() or matches[0].size > 65536:
            raise ValueError("Expected one small BUILD-INFO.json in the Linux archive")
        with stream.extractfile(matches[0]) as content:
            info = json.load(content)
    if info.get("target") != "x86_64-unknown-linux-gnu" or info.get("capture") is not False:
        raise ValueError("The desktop requires a normal Linux x64 package without capture")
    if not re.fullmatch(r"[a-f0-9]{40}", info.get("source_commit", "")):
        raise ValueError("Invalid package source commit")
    return info


def save_environment(report):
    REPORTS.mkdir(parents=True, exist_ok=True)
    # Keep a complete report if a signal arrives while updating it.
    with tempfile.NamedTemporaryFile("w", dir=REPORTS, prefix=".environment-",
                                     suffix=".json", encoding="utf-8", delete=False) as stream:
        temporary = Path(stream.name)
        json.dump(report, stream, indent=2)
        stream.write("\n")
    try:
        temporary.replace(REPORTS / "environment.json")
    finally:
        temporary.unlink(missing_ok=True)


def desktop_environment(backend):
    env = dict(os.environ)
    env.setdefault("HOME", "/home/tester")
    env.setdefault("XDG_RUNTIME_DIR", "/tmp/rastera-runtime")
    env.setdefault("XDG_DATA_HOME", str(Path(env["HOME"]) / ".local/share"))
    env.setdefault("XDG_CACHE_HOME", str(Path(env["HOME"]) / ".cache"))
    env.setdefault("XDG_CONFIG_HOME", str(Path(env["HOME"]) / ".config"))
    env.setdefault("XDG_CURRENT_DESKTOP", "GNOME")
    env["PYTHONDONTWRITEBYTECODE"] = "1"
    for key in ("HOME", "XDG_RUNTIME_DIR", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_CONFIG_HOME"):
        path = Path(env[key])
        if not path.is_absolute():
            raise ValueError(key + " must be an absolute path")
        path.mkdir(parents=True, exist_ok=True)
    runtime = Path(env["XDG_RUNTIME_DIR"])
    if runtime.is_symlink() or runtime.stat().st_uid != os.getuid():
        raise ValueError("XDG_RUNTIME_DIR must be a directory owned by the test user")
    runtime.chmod(0o700)
    icds = sorted(Path("/usr/share/vulkan/icd.d").glob("lvp_icd*.json"))
    if not icds:
        raise FileNotFoundError("Mesa's lavapipe Vulkan ICD is required for container testing")
    env["VK_ICD_FILENAMES"] = env["VK_DRIVER_FILES"] = str(icds[0])
    # Do not let developer instrumentation variables change the test scenario.
    for key in list(env):
        if key.startswith("RASTERA_CAPTURE_") or key == "RASTERA_NAVIGATION_BENCHMARK":
            env.pop(key)
    env["RASTERA_DISPLAY_BACKEND"] = backend
    env["XDG_SESSION_TYPE"] = backend
    env["GDK_BACKEND"] = backend
    env["WINIT_UNIX_BACKEND"] = backend
    # Never inherit another compositor's socket or an X11 fallback in Wayland mode.
    env.pop("WAYLAND_SOCKET", None)
    if backend == "wayland":
        env.pop("DISPLAY", None)
        env.pop("XAUTHORITY", None)
        env["WAYLAND_DISPLAY"] = WAYLAND_SOCKET
        env["WAYLAND_DEBUG"] = "client"
    else:
        env.setdefault("DISPLAY", ":99")
        env.pop("WAYLAND_DISPLAY", None)
        env.pop("WAYLAND_DEBUG", None)
    return env


class Runtime:
    def __init__(self, env):
        self.env = env
        self.children = []
        self.stopping = False
        self.exit_code = 0
        self.failed_service = None

    def on_signal(self, number, _frame):
        self.stopping = True
        self.exit_code = 128 + number

    def start(self, name, command, cwd=None, env=None):
        if self.stopping:
            raise InterruptedError("Container shutdown requested")
        log = (REPORTS / (name + ".log")).open("a", encoding="utf-8")
        log.write("\n[" + now() + "] Starting " + name + "\n")
        log.flush()
        try:
            process = subprocess.Popen(command, cwd=cwd, env=env if env is not None else self.env,
                                       stdout=log,
                                       stderr=subprocess.STDOUT, start_new_session=True)
        except BaseException:
            log.close()
            raise
        self.children.append((name, process, log))
        return process

    def wait(self, process, required=()):
        while not self.stopping:
            code = process.poll()
            if code is not None:
                return code
            self.check_services(required)
            time.sleep(0.2)
        return self.exit_code

    def check_services(self, required):
        for name, process in required:
            code = process.poll()
            if code is not None:
                signal_name = None
                if code < 0:
                    with contextlib.suppress(ValueError):
                        signal_name = signal.Signals(-code).name
                self.failed_service = {"name": name, "pid": process.pid,
                                       "exit_code": code, "signal": signal_name}
                status = "exit code " + str(code)
                if signal_name:
                    status += " (" + signal_name + ")"
                raise RuntimeError(name + " exited with " + status +
                                   "; see /reports/" + name + ".log")

    def close(self):
        # Each process has its own session. Stop only groups this entrypoint created,
        # including descendants left behind by a terminated check harness.
        children, self.children = self.children, []
        for _, process, _ in reversed(children):
            with contextlib.suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGTERM)
        deadline = time.monotonic() + 5
        for _, process, _ in reversed(children):
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=max(0, deadline - time.monotonic()))
        for _, process, log in reversed(children):
            with contextlib.suppress(ProcessLookupError):
                os.killpg(process.pid, signal.SIGKILL)
            with contextlib.suppress(subprocess.TimeoutExpired):
                process.wait(timeout=2)
            log.write("[" + now() + "] Process " + str(process.pid) +
                      " exit code: " + str(process.returncode) + "\n")
            log.close()


def wait_window_manager(runtime, services, env):
    """Wait for Openbox's live EWMH window and advertised fullscreen support."""
    deadline = time.monotonic() + 10
    window_property = r"_NET_SUPPORTING_WM_CHECK\(WINDOW\):\s*window id #\s*(0x[0-9a-fA-F]+)"
    with (REPORTS / "openbox-readiness.log").open("a", encoding="utf-8") as log:
        while not runtime.stopping and time.monotonic() < deadline:
            runtime.check_services(services)
            try:
                root = subprocess.run(["xprop", "-display", env["DISPLAY"], "-root",
                                       "_NET_SUPPORTING_WM_CHECK", "_NET_SUPPORTED"],
                                      env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                      text=True, timeout=1)
                log.write("[" + now() + "] Root EWMH properties\n" + root.stdout)
                supporting = re.search(window_property, root.stdout)
                if root.returncode == 0 and supporting and re.search(
                        r"\b_NET_WM_STATE_FULLSCREEN\b", root.stdout):
                    window = supporting.group(1)
                    check = subprocess.run(["xprop", "-display", env["DISPLAY"], "-id", window,
                                            "_NET_SUPPORTING_WM_CHECK", "_NET_WM_NAME"],
                                           env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                           text=True, timeout=1)
                    log.write("[" + now() + "] Supporting WM window\n" + check.stdout)
                    own_window = re.search(window_property, check.stdout)
                    if (check.returncode == 0 and own_window and
                            int(own_window.group(1), 16) == int(window, 16) and
                            re.search(r'_NET_WM_NAME\([^)]*\)\s*=\s*"Openbox"', check.stdout)):
                        runtime.check_services(services)
                        log.write("[" + now() + "] Openbox ready; EWMH fullscreen advertised\n")
                        return
                log.flush()
            except subprocess.TimeoutExpired:
                log.write("[" + now() + "] EWMH query timed out\n")
                log.flush()
            time.sleep(0.1)
    if runtime.stopping:
        raise InterruptedError("Container shutdown requested")
    raise RuntimeError("Openbox did not advertise a live EWMH window and fullscreen support; "
                       "see /reports/openbox-readiness.log and openbox.log")


def start_x11_display(runtime, env):
    display = env["DISPLAY"]
    if not re.fullmatch(r":\d+", display):
        raise ValueError("DISPLAY must identify a local X server, such as :99")
    server = runtime.start("xvfb", ["Xvfb", display, "-screen", "0", "1440x1000x24",
                                    "-nolisten", "tcp", "-s", "0"], env=env)
    deadline = time.monotonic() + 15
    while not runtime.stopping and time.monotonic() < deadline:
        runtime.check_services((("xvfb", server),))
        try:
            probe = subprocess.run(["xdpyinfo", "-display", display], env=env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1)
            if probe.returncode == 0:
                window_manager = runtime.start("openbox", ["openbox", "--sm-disable"], env=env)
                services = (("xvfb", server), ("openbox", window_manager))
                wait_window_manager(runtime, services, env)
                return services
        except subprocess.TimeoutExpired:
            pass
        time.sleep(0.2)
    raise RuntimeError("Xvfb did not become ready; see /reports/xvfb.log")


def compositor_environment(runtime, nested=False):
    env = dict(runtime.env)
    for key in ("WAYLAND_DISPLAY", "WAYLAND_DEBUG", "WAYLAND_SOCKET", "WINIT_UNIX_BACKEND"):
        env.pop(key, None)
    if nested:
        env["DISPLAY"] = os.environ.get("DISPLAY", ":99")
        env["GDK_BACKEND"] = "x11"
        env["XDG_SESSION_TYPE"] = "x11"
        env["LIBGL_ALWAYS_SOFTWARE"] = "1"
        env["GALLIUM_DRIVER"] = "llvmpipe"
    return env


def weston_command(env, nested):
    version = subprocess.run(["weston", "--version"], env=env, check=True,
                             capture_output=True, text=True, timeout=5).stdout.strip()
    help_text = subprocess.run(["weston", "--help"], env=env, check=True,
                               capture_output=True, text=True, timeout=5).stdout
    modern_renderer = "--renderer=NAME" in help_text
    # Weston 13's nested X11/pixman renderer crashed with SIGSEGV in this
    # container. Its GL renderer is still CPU-only through Mesa llvmpipe.
    # Keep the independently validated headless pixman configuration unchanged.
    renderer = ("--renderer=gl" if nested else "--renderer=pixman") if modern_renderer else "--use-pixman"
    if renderer == "--use-pixman" and "--use-pixman" not in help_text:
        raise RuntimeError("Weston does not advertise a software pixman renderer")
    backend = "x11" if nested else "headless"
    # Weston 9/10 use module names; Weston 13 advertises short backend names.
    backend_option = backend + "-backend.so" if backend + "-backend.so" in help_text else backend
    command = ["weston", "--backend=" + backend_option, renderer,
               "--socket=" + WAYLAND_SOCKET, "--width=1440", "--height=1000",
               "--idle-time=0", "--no-config"]
    if nested:
        command.append("--fullscreen")
    return command, version


def start_wayland_display(runtime, mode, environment):
    nested = mode == "desktop"
    env = compositor_environment(runtime, nested)
    services = start_x11_display(runtime, env) if nested else ()
    command, version = weston_command(env, nested)
    socket_path = Path(runtime.env["XDG_RUNTIME_DIR"]) / WAYLAND_SOCKET
    if socket_path.exists() or socket_path.is_symlink():
        raise RuntimeError("The test Wayland socket already exists: " + str(socket_path))
    compositor = runtime.start("weston", command, env=env)
    services += (("weston", compositor),)
    diagnostic = shutil.which("wayland-info") or shutil.which("weston-info")
    if not diagnostic:
        raise FileNotFoundError("Install wayland-info or Weston's older weston-info diagnostic")
    probe_env = dict(runtime.env)
    probe_env.pop("WAYLAND_DEBUG", None)
    deadline = time.monotonic() + 25
    with (REPORTS / "wayland-readiness.log").open("a", encoding="utf-8") as log:
        while not runtime.stopping and time.monotonic() < deadline:
            runtime.check_services(services)
            try:
                socket_stat = socket_path.stat()
                if not stat.S_ISSOCK(socket_stat.st_mode) or socket_stat.st_uid != os.getuid():
                    raise RuntimeError("The test compositor must expose a user-owned Unix socket")
                probe = subprocess.run([diagnostic], env=probe_env, stdout=log,
                                       stderr=subprocess.STDOUT, timeout=3)
                if probe.returncode == 0:
                    break
            except (FileNotFoundError, subprocess.TimeoutExpired):
                pass
            time.sleep(0.2)
        else:
            raise RuntimeError("Weston did not become ready; see /reports/weston.log "
                               "and wayland-readiness.log")
    if runtime.stopping:
        raise InterruptedError("Container shutdown requested")
    environment["compositor"] = {
        "name": "Weston", "version": version, "backend": "x11-nested" if nested else "headless",
        "renderer": "GL (Mesa llvmpipe software)" if "--renderer=gl" in command else "pixman (software)",
        "command": command, "wayland_socket": str(socket_path),
        "readiness_probe": Path(diagnostic).name, "parent_display": env.get("DISPLAY"),
        "xwayland_enabled": False,
    }
    # D-Bus-activated GTK/portal processes must use this compositor too. Empty X11
    # values clear any DISPLAY inherited by dbus-run-session before this entrypoint.
    activation = runtime.start("portal-environment", ["dbus-update-activation-environment",
        "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "XDG_CURRENT_DESKTOP", "XDG_SESSION_TYPE",
        "GDK_BACKEND", "DISPLAY=", "XAUTHORITY="])
    if runtime.wait(activation, services) != 0:
        raise RuntimeError("Could not update the D-Bus activation environment for Wayland")
    return services


def start_display(runtime, mode, environment):
    if runtime.env["RASTERA_DISPLAY_BACKEND"] == "wayland":
        return start_wayland_display(runtime, mode, environment)
    environment["compositor"] = {"name": "Openbox", "backend": "x11",
                                 "parent_display": runtime.env["DISPLAY"]}
    return start_x11_display(runtime, runtime.env)


def verifier_module():
    sys.path.insert(0, str(SCRIPTS))
    spec = importlib.util.spec_from_file_location("rastera_unix_verifier", SCRIPTS / "verify-unix-package.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def copy_practice_images(package, practice):
    practice.mkdir(parents=True, exist_ok=True)
    samples = sorted((package / "docs/tutorial/ejemplos").glob("*.png"))
    if not samples:
        raise ValueError("The package must include the tutorial's PNG practice images")
    for source in samples:
        try:
            with (practice / source.name).open("xb") as target, source.open("rb") as original:
                shutil.copyfileobj(original, target)
        except FileExistsError:
            # A persistent HOME may contain a user's modified copy from a prior run.
            continue


def run_desktop(runtime, archive, expected_commit, environment, services):
    report_path = REPORTS / "package-verification.json"
    command = [sys.executable, str(SCRIPTS / "verify-unix-package.py"), "--archive", str(archive),
               "--report", str(report_path)]
    if expected_commit:
        command += ["--expected-commit", expected_commit]
    verification = runtime.start("package-verification", command)
    code = runtime.wait(verification, services)
    if code:
        return code
    with report_path.open(encoding="utf-8") as stream:
        verified = json.load(stream)
    if verified.get("result") != "passed" or verified.get("native_checked") is not True:
        raise ValueError("Native package verification did not pass")
    with tempfile.TemporaryDirectory(prefix="rastera-desktop-") as temporary:
        extracted = Path(temporary) / "extracted"
        extracted.mkdir()
        package = verifier_module().safe_extract(archive, extracted)
        practice = Path(runtime.env["HOME"]) / "Pictures/Rastera-practice"
        exports = Path(runtime.env["HOME"]) / "Pictures/Rastera-exports"
        copy_practice_images(package, practice)
        exports.mkdir(parents=True, exist_ok=True)
        executable = package / "bin/rastera"
        host_port = 6081 if runtime.env["RASTERA_DISPLAY_BACKEND"] == "wayland" else 6080
        desktop_url = f"http://localhost:{host_port}/vnc.html"
        environment.update({"package_verified": True, "extracted_package": str(package),
                            "executable": str(executable), "practice_folder": str(practice),
                            "export_folder": str(exports), "novnc_url": desktop_url})
        save_environment(environment)
        app = runtime.start("rastera", [str(executable), str(practice)], cwd=package)
        vnc_env = (compositor_environment(runtime, nested=True)
                   if runtime.env["RASTERA_DISPLAY_BACKEND"] == "wayland" else runtime.env)
        vnc = runtime.start("x11vnc", ["x11vnc", "-display", vnc_env["DISPLAY"], "-localhost",
                                      "-rfbport", "5900", "-nopw", "-forever", "-shared"], env=vnc_env)
        websocket = runtime.start("websockify", ["websockify", "--web", "/usr/share/novnc",
                                                 "6080", "localhost:5900"])
        required = services + (("x11vnc", vnc), ("websockify", websocket))
        print("Rastera desktop ready at " + desktop_url, flush=True)
        app_exit_reported = False
        while not runtime.stopping:
            runtime.check_services(required)
            if app.poll() is not None and not app_exit_reported:
                environment["application_exit_code"] = app.returncode
                save_environment(environment)
                print("Rastera exited; the desktop remains available until container shutdown.", flush=True)
                app_exit_reported = True
            time.sleep(0.3)
        # Stop the app before removing the temporary executable directory.
        runtime.close()
    return runtime.exit_code


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=("check", "desktop"), default=os.environ.get("RASTERA_MODE", "check"))
    parser.add_argument("--display-backend", choices=("x11", "wayland"),
                        default=os.environ.get("RASTERA_DISPLAY_BACKEND", "x11"))
    args = parser.parse_args()
    if args.mode not in ("check", "desktop"):
        parser.error("RASTERA_MODE must be check or desktop")
    if args.display_backend not in ("x11", "wayland"):
        parser.error("RASTERA_DISPLAY_BACKEND must be x11 or wayland")
    if not os.environ.get("RASTERA_ARCHIVE"):
        raise ValueError("Set RASTERA_ARCHIVE to the mounted Linux release archive")
    archive = Path(os.environ["RASTERA_ARCHIVE"]).resolve(strict=True)
    expected_commit = os.environ.get("RASTERA_EXPECTED_COMMIT") or None
    if expected_commit and not re.fullmatch(r"[a-f0-9]{40}", expected_commit):
        raise ValueError("RASTERA_EXPECTED_COMMIT must be a full lowercase Git SHA")
    info = archive_metadata(archive)
    env = desktop_environment(args.display_backend)
    if args.display_backend == "wayland":
        session = "Wayland with nested X11 compositor" if args.mode == "desktop" else "headless Wayland"
        scope = ("Container " + session + " and software Vulkan; clients configured without "
                 "DISPLAY, Xwayland disabled. Hardware GPU, full desktop sessions and "
                 "accessibility need separate validation")
    else:
        scope = ("Container X11 and software Vulkan; hardware GPU, Wayland, clean desktop "
                 "and accessibility need separate validation")
    environment = {
        "started_utc": now(), "mode": args.mode, "archive": str(archive),
        "archive_sha256": sha256(archive), "source_commit": info["source_commit"],
        "expected_commit": expected_commit, "version": info.get("version"),
        "target": info["target"], "capture": info["capture"], "package_verified": False,
        "uid": os.getuid(), "gid": os.getgid(), "python": sys.version.split()[0],
        "system": list(os.uname()), "display_backend": args.display_backend,
        "display": env.get("DISPLAY"), "wayland_display": env.get("WAYLAND_DISPLAY"),
        "wayland_debug": env.get("WAYLAND_DEBUG"), "screen": "1440x1000x24",
        "vulkan_driver": "Mesa lavapipe (software)", "vulkan_icd": env["VK_DRIVER_FILES"],
        "paths": {key: env[key] for key in ("HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                                           "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR")},
        "catalog": str(Path(env["XDG_DATA_HOME"]) / "Rastera/catalog.sqlite"),
        "thumbnail_cache": str(Path(env["XDG_CACHE_HOME"]) / "Rastera/thumbnails-v1"),
        "scope": scope,
    }
    save_environment(environment)
    runtime = Runtime(env)
    for number in (signal.SIGTERM, signal.SIGINT):
        signal.signal(number, runtime.on_signal)
    try:
        services = start_display(runtime, args.mode, environment)
        save_environment(environment)
        if args.mode == "desktop":
            code = run_desktop(runtime, archive, expected_commit, environment, services)
        else:
            command = [sys.executable, str(SCRIPTS / "docker/test-linux-alpha.py"), "--archive", str(archive),
                       "--report-dir", str(REPORTS), "--gui"]
            command += ["--expected-commit", expected_commit or info["source_commit"]]
            check = runtime.start("linux-check", command)
            code = runtime.wait(check, services)
            environment["check_exit_code"] = code
            environment["package_verified"] = code == 0
        environment["finished_utc"] = now()
        save_environment(environment)
        return code
    except BaseException as error:
        environment["finished_utc"] = now()
        if runtime.failed_service:
            environment["failed_service"] = runtime.failed_service
        if runtime.stopping:
            environment["shutdown_exit_code"] = runtime.exit_code
            save_environment(environment)
            return runtime.exit_code
        environment["error"] = str(error)
        save_environment(environment)
        raise
    finally:
        runtime.close()


if __name__ == "__main__":
    try:
        sys.exit(main())
    except Exception as error:
        print("Rastera test container: " + str(error), file=sys.stderr)
        sys.exit(1)
