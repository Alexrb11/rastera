# Test Linux packages with Docker Desktop

This harness checks an already built Linux x86-64 release archive in Ubuntu 22.04, Ubuntu 24.04 and Debian 12. The containers contain desktop runtime libraries and test tools, without Rust or Cargo. Rastera runs as the unprivileged `tester` user, so the tests exercise the distributed application and per-user installer.

The automated suite and the optional browser desktop have different purposes. The suite records assertions and logs; the desktop allows manual inspection of Rastera's normal interface. Neither requires the application's development-only `capture` feature.

## Recorded local validation

On 2026-10-11, the three local runtime services passed all 11 driver checks against the same `0.1.0-alpha.3` package:

| Runtime | Driver result |
| --- | --- |
| Ubuntu 22.04 | 11/11 checks passed |
| Ubuntu 24.04 | 11/11 checks passed |
| Debian 12 | 11/11 checks passed |

Archive: `rastera-0.1.0-alpha.3-linux-x64.tar.gz`. Source commit: `382b55a19221618d10af4a4cdf94e97062408f50`. SHA-256: `db98d3972b7afdb28f4409481129ad0843420bb5eb8103e6a8826cd8c0717ef3`.

These runs used Docker Desktop's `6.18.40.1-microsoft-standard-WSL2` kernel, X11/Xvfb and Mesa lavapipe software Vulkan. The distributions supplied different user-space libraries; they did not run independent distribution kernels.

Separate interaction with the normal Ubuntu 24.04 browser desktop confirmed:

- The thumbnail grid, favorite/tag editing through the UI, search and image viewer.
- Automatic library updates after file creation, modification, renaming and removal without F5.
- Batch export of three JPEGs at 512 × 512 pixels, verified by decoding the outputs and checking the completed journal with three published files. Repeating the export showed the existing-output error; output hashes and the number of journal executions stayed unchanged. Original image hashes were preserved.
- Ctrl+O opening the GTK portal file chooser and the selected image decoding correctly in the viewer.
- Normal application shutdown with Alt+F4 and exit code 0, followed by container recreation using the same HOME volume. The favorite checkbox and tags were still visible in the UI, and SQLite retained their values and image dimensions. Original and exported image hashes remained unchanged.

These manual observations are separate from the automated persistence checks below, which use seeded SQLite fixtures. Zoom, window resizing, batch cancellation and export formats other than JPEG were not exercised in this manual session.

## Requirements

Use Docker Desktop with its Linux container engine running, Docker Compose v2, and PowerShell for the Windows runner. Docker Desktop's [Windows setup guide](https://docs.docker.com/desktop/setup/install/windows-install/) describes the supported Linux backends and host requirements. From the repository root, check both the client and the engine:

```powershell
docker compose version
docker info --format '{{.OSType}}'
```

The second command must succeed and print `linux`. An installed Docker CLI does not mean the engine is ready. Complete Docker Desktop's first-run setup and wait for the engine before starting tests. Building the test images requires internet access to download the distribution image and runtime packages.

Obtain `rastera-0.1.0-alpha.3-linux-x64.tar.gz` and its matching `.sha256` sidecar from the same release or Actions artifact. Keep them together. The archive must contain a normal Linux build with `capture=false`; a Windows ZIP or a development capture binary is unsuitable. The runner verifies the archive rather than compiling Rastera inside the containers.

## Run the automated matrix

```powershell
.\scripts\test-linux-containers.ps1 `
  -Archive ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz" `
  -ReportsDirectory ".\work\linux-container-reports"
```

The default services are `ubuntu2204`, `ubuntu2404` and `debian12`, defined in [compose.yaml](../scripts/docker/compose.yaml). To test one environment:

```powershell
.\scripts\test-linux-containers.ps1 `
  -Archive ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz" `
  -ReportsDirectory ".\work\linux-ubuntu2404-reports" `
  -Service ubuntu2404
```

Only the repository's `scripts` directory is mounted at `/workspace/scripts` read-only. The archive directory is mounted at `/release` read-only, and each service's report subfolder at `/reports` with write access. Images and user profiles used by the driver are temporary copies inside the container. The automated containers have networking disabled at runtime and are limited to two CPUs and 2 GiB of memory. Docker's [bind mount documentation](https://docs.docker.com/engine/storage/bind-mounts/) explains the mount options.

Use `-ExpectedCommit` with a full lowercase Git SHA to require a particular archive source revision. Without that option, the container entrypoint reads the archive's declared source commit and passes it to the verifier and driver; this checks consistency with package metadata. Supply the option when checking against an independently known revision. Desktop mode also checks native package metadata and applies an exact revision check when the option is supplied. The current driver scenario targets `0.1.0-alpha.3`; review its version fixtures before testing a different release. Direct invocation of `test-linux-alpha.py` without `--expected-commit` retains the recorded alpha.3 source commit as its default.

[test-linux-alpha.py](../scripts/docker/test-linux-alpha.py) checks:

- Archive checksum, safe extraction, build metadata, architecture and native dependencies through the package verifier.
- An unprivileged runtime with no `rustc` or `cargo` on PATH.
- Normal GUI startup on X11/Xvfb, opening a temporary folder with one practice JPEG, decoding it and recording its dimensions in SQLite.
- The real per-user installer in a temporary HOME containing spaces, launcher startup, refusal to reinstall the same version, and replacement of an explicitly declared earlier-installation fixture.
- Persistence of a favorite and tag seeded while Rastera is closed, across portable and installed application launches.
- MIME registration without replacing existing default handlers, and unchanged hashes of the copied practice images.

The earlier-installation fixture checks ownership and replacement behavior; it is not an upgrade test against a historical Rastera binary. The annotation fixture checks persistence, rather than editing favorites or tags through the UI.

## Run the matrix in GitHub Actions

The manual [Linux release containers workflow](../.github/workflows/linux-containers.yml) runs the same three runtime services without a Rust toolchain in the images. In **Actions → Linux release containers → Run workflow**, supply `package_run_id`, the ID of a completed, successful **Desktop alpha** packaging run in this repository containing `linux-x64-alpha`. The workflow checks that run's repository, workflow and outcome before downloading its artifact, and verifies the package against that run's source commit.

Each service uploads a `linux-container-<service>` report artifact, including on test failure when reports exist. This workflow does not start the interactive noVNC desktop. Its outcome is separate from the local matrix results recorded above.

## Inspect reports

Each tested service writes `results.json` with its outcome, archive SHA-256, source commit, environment and individual checks. It also writes `native-verification.json` and logs for native verification, installation, refused reinstallation and replacement. GUI logs are `portable-gui.log`, `installed-gui.log` and `upgrade-gui.log`; a failed driver run adds `failure.log`. The host runner records `image.json` with the built image ID and Compose-file hash. `environment.json`, `linux-check.log`, `xvfb.log` and `openbox.log` describe the container's display and execution setup.

A passing process exit and passing recorded assertions establish the checks listed above. Keep the JSON and relevant logs when reporting a failure, together with the host's Docker/Compose versions. Check the recorded source commit and archive hash before comparing different runs. Do not include private images or user profiles in an issue.

## Open the browser desktop

The optional `desktop` Compose profile uses Ubuntu 24.04, Xvfb, Openbox and the distribution's noVNC/websockify packages. Start it separately from the matrix:

```powershell
.\scripts\test-linux-containers.ps1 `
  -Archive ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz" `
  -ReportsDirectory ".\work\linux-desktop-reports" `
  -Desktop
```

Open [the local desktop](http://127.0.0.1:6080/vnc.html?autoconnect=true&resize=scale) in a browser. The port is bound to loopback and the local viewer has no password. The profile stays running for manual checks; reports for this mode are stored under the report directory's `desktop` subfolder. Compose [profiles](https://docs.docker.com/compose/how-tos/profiles/) keep this interactive service separate from the default matrix.

Use F4 in Rastera to open `/home/tester/Pictures/Rastera-practice`. The interface is currently in Spanish. Useful manual checks are:

1. Browse the thumbnail grid, open an image, zoom and resize the window.
2. Use **Organizar selección** to add a favorite and tag; close and reopen Rastera and check that they persist.
3. Export selected images to `/home/tester/Pictures/Rastera-exports` and inspect formats, dimensions, cancellation and refusal to overwrite existing outputs.
4. Add, modify, rename and remove practice files outside Rastera and observe the library's automatic updates.
5. Open the file/folder dialogs and check keyboard shortcuts, focus and multiple selection.

These manual checks need their own observations or screenshots. The automated driver does not assert folder-observer behavior, UI editing, batch export or visual correctness. Desktop screenshots are separate from the application's capture builds and from the suite's JSON results.

## Desktop logs, screenshots and shutdown

The runner restores its environment variables when it exits. Before using Compose directly, set the same archive and report locations used to start the desktop:

```powershell
$archive = (Resolve-Path ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz").Path
$env:RASTERA_PACKAGE_DIR = (Split-Path -Parent $archive).Replace('\', '/')
$env:RASTERA_ARCHIVE_NAME = Split-Path -Leaf $archive
$env:RASTERA_REPORTS_DIR = (Resolve-Path ".\work\linux-desktop-reports").Path.Replace('\', '/')
$env:RASTERA_EXPECTED_COMMIT = ''

docker compose -f .\scripts\docker\compose.yaml --profile desktop logs desktop
docker compose -f .\scripts\docker\compose.yaml --profile desktop exec -T desktop import -display :99 -window root /reports/desktop-review.png
```

The screenshot appears on the host as `desktop/desktop-review.png`. Desktop mode also records `package-verification.json`, `package-verification.log`, `environment.json`, `rastera.log`, `x11vnc.log` and `websockify.log`. If Rastera is closed, the browser desktop remains running. To restart its normal app with the same catalog, or stop the environment:

```powershell
docker compose -f .\scripts\docker\compose.yaml --profile desktop restart desktop
docker compose -f .\scripts\docker\compose.yaml --profile desktop down
```

The `desktop-home` named volume retains the test user's practice copies, catalog and exports across container restarts and normal `down`. To start with a fresh test profile, use `down --volumes`; this removes those test data from the volume. Host report files remain in the chosen report directory.

## What this environment establishes

The three distribution images exercise different Linux user-space libraries while sharing Docker Desktop's Linux kernel. Xvfb supplies an X11 virtual display and Mesa supplies software Vulkan. Results therefore cover these runtime combinations and packaged application behavior, with no Rust development environment present.

They do not establish compatibility with macOS, physical GPUs or their drivers, native Wayland compositors, every desktop portal, HiDPI configurations, accessibility tools, network filesystems or clean installations on independent Linux kernels. Test those on real target desktops before declaring broader support. The [Linux installation guide](install-linux.md) lists the application's runtime requirements and current experimental scope.
