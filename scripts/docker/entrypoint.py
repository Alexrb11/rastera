"""Run native Linux checks or an interactive X11 desktop with the normal release."""
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
import subprocess
import sys
import tarfile
import tempfile
import time


SCRIPTS = Path("/workspace/scripts")
REPORTS = Path("/reports")


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


def desktop_environment():
    env = dict(os.environ)
    env.setdefault("HOME", "/home/tester")
    env.setdefault("DISPLAY", ":99")
    env.setdefault("XDG_RUNTIME_DIR", "/tmp/rastera-runtime")
    env.setdefault("XDG_DATA_HOME", str(Path(env["HOME"]) / ".local/share"))
    env.setdefault("XDG_CACHE_HOME", str(Path(env["HOME"]) / ".cache"))
    env.setdefault("XDG_CONFIG_HOME", str(Path(env["HOME"]) / ".config"))
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
    return env


class Runtime:
    def __init__(self, env):
        self.env = env
        self.children = []
        self.stopping = False
        self.exit_code = 0

    def on_signal(self, number, _frame):
        self.stopping = True
        self.exit_code = 128 + number

    def start(self, name, command, cwd=None):
        if self.stopping:
            raise InterruptedError("Container shutdown requested")
        log = (REPORTS / (name + ".log")).open("a", encoding="utf-8")
        log.write("\n[" + now() + "] Starting " + name + "\n")
        log.flush()
        try:
            process = subprocess.Popen(command, cwd=cwd, env=self.env, stdout=log,
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
            if process.poll() is not None:
                raise RuntimeError(name + " exited; see /reports/" + name + ".log")

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
            log.close()


def start_display(runtime):
    display = runtime.env["DISPLAY"]
    if not re.fullmatch(r":\d+", display):
        raise ValueError("DISPLAY must identify a local X server, such as :99")
    server = runtime.start("xvfb", ["Xvfb", display, "-screen", "0", "1440x1000x24",
                                    "-nolisten", "tcp", "-s", "0"])
    deadline = time.monotonic() + 15
    while not runtime.stopping and time.monotonic() < deadline:
        runtime.check_services((("xvfb", server),))
        try:
            probe = subprocess.run(["xdpyinfo", "-display", display], env=runtime.env,
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=1)
            if probe.returncode == 0:
                window_manager = runtime.start("openbox", ["openbox", "--sm-disable"])
                return (("xvfb", server), ("openbox", window_manager))
        except subprocess.TimeoutExpired:
            pass
        time.sleep(0.2)
    raise RuntimeError("Xvfb did not become ready; see /reports/xvfb.log")


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
        environment.update({"package_verified": True, "extracted_package": str(package),
                            "executable": str(executable), "practice_folder": str(practice),
                            "export_folder": str(exports), "novnc_url": "http://localhost:6080/vnc.html"})
        save_environment(environment)
        app = runtime.start("rastera", [str(executable), str(practice)], cwd=package)
        vnc = runtime.start("x11vnc", ["x11vnc", "-display", runtime.env["DISPLAY"], "-localhost",
                                      "-rfbport", "5900", "-nopw", "-forever", "-shared"])
        websocket = runtime.start("websockify", ["websockify", "--web", "/usr/share/novnc",
                                                 "6080", "localhost:5900"])
        required = services + (("x11vnc", vnc), ("websockify", websocket))
        print("Rastera desktop ready at http://localhost:6080/vnc.html", flush=True)
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
    args = parser.parse_args()
    if args.mode not in ("check", "desktop"):
        parser.error("RASTERA_MODE must be check or desktop")
    if not os.environ.get("RASTERA_ARCHIVE"):
        raise ValueError("Set RASTERA_ARCHIVE to the mounted Linux release archive")
    archive = Path(os.environ["RASTERA_ARCHIVE"]).resolve(strict=True)
    expected_commit = os.environ.get("RASTERA_EXPECTED_COMMIT") or None
    if expected_commit and not re.fullmatch(r"[a-f0-9]{40}", expected_commit):
        raise ValueError("RASTERA_EXPECTED_COMMIT must be a full lowercase Git SHA")
    info = archive_metadata(archive)
    env = desktop_environment()
    environment = {
        "started_utc": now(), "mode": args.mode, "archive": str(archive),
        "archive_sha256": sha256(archive), "source_commit": info["source_commit"],
        "expected_commit": expected_commit, "version": info.get("version"),
        "target": info["target"], "capture": info["capture"], "package_verified": False,
        "uid": os.getuid(), "gid": os.getgid(), "python": sys.version.split()[0],
        "system": list(os.uname()), "display": env["DISPLAY"], "screen": "1440x1000x24",
        "vulkan_driver": "Mesa lavapipe (software)", "vulkan_icd": env["VK_DRIVER_FILES"],
        "paths": {key: env[key] for key in ("HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME",
                                           "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR")},
        "catalog": str(Path(env["XDG_DATA_HOME"]) / "Rastera/catalog.sqlite"),
        "thumbnail_cache": str(Path(env["XDG_CACHE_HOME"]) / "Rastera/thumbnails-v1"),
        "scope": "Container X11 and software Vulkan; hardware GPU, Wayland, clean desktop and accessibility need separate validation",
    }
    save_environment(environment)
    runtime = Runtime(env)
    for number in (signal.SIGTERM, signal.SIGINT):
        signal.signal(number, runtime.on_signal)
    try:
        services = start_display(runtime)
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
