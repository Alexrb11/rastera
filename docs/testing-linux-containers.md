# Test Linux packages with Docker Desktop

This harness checks an already built Linux x86-64 release archive in Ubuntu 22.04, Ubuntu 24.04 and Debian 12, using either X11 or native Wayland clients. The containers contain desktop runtime libraries and test tools, without Rust or Cargo. Rastera runs as the unprivileged `tester` user, so the tests exercise the distributed application and per-user installer.

The automated suite and the optional browser desktop have different purposes. The suite records assertions and logs; the desktop allows manual inspection of Rastera's normal interface. Neither requires the application's development-only `capture` feature.

## Recorded six-service validation

On 2026-10-11, the rebuilt alpha.3 Linux archive passed **66 local driver checks and 66 checks in GitHub Actions: 11 in each of six services per matrix**, covering X11/Xvfb and native Wayland clients on headless Weston. The historical X11 results remain tied to their original archive; both builds carry the same alpha.3 version and filename but have different source commits and checksums.

Archive: `rastera-0.1.0-alpha.3-linux-x64.tar.gz`. Application source commit: `cf56c2efcba273e4508cc94119a71b374b0ddf54`. SHA-256: `080db2d854c26ab288d24cb549cff02adab548e14f1d63c95177a17115f7f4f3`. The prepared Linux artifact came from the successful second attempt of [Desktop alpha packaging run 38156421221](https://github.com/Alexrb11/rastera/actions/runs/38156421221). The final local matrix used harness commit `267a8180f465f6641d8b0fe7ca124139f74b630e`, including the boot-specific tmpfs configuration; the final CI used `08857c54cbbb9738806ff610036b97700003fc35`. Local reports record image IDs and Compose/entrypoint checksums. Package source, harness source and individual workflow outcomes are separate provenance.

| Service | Display setup | Local | Final CI run 38158919754 |
| --- | --- | --- | --- |
| `ubuntu2204` | X11/Xvfb | 11/11 passed | 11/11 passed |
| `ubuntu2404` | X11/Xvfb | 11/11 passed | 11/11 passed |
| `debian12` | X11/Xvfb | 11/11 passed | 11/11 passed |
| `ubuntu2204-wayland` | Headless Weston 9.0.0, pixman | 11/11 passed | 11/11 passed |
| `ubuntu2404-wayland` | Headless Weston 13.0.0, pixman | 11/11 passed | 11/11 passed |
| `debian12-wayland` | Headless Weston 10.0.1, pixman | 11/11 passed | 11/11 passed |

Each matrix included nine Wayland launches (portable, installed and replacement on each distribution) that decoded the example JPEG and recorded native client protocol evidence. The trace links the expected application ID `io.github.Alexrb11.rastera` to its own `xdg_toplevel`, `xdg_surface` and `wl_surface`, and verifies a configure/acknowledge pair with the same serial. This records a configured Rastera window with a non-null buffer committed to its own surface. GUI-log SHA-256 values are recorded in each service's `results.json`. `DISPLAY` was absent, Xwayland was disabled, and no X11 fallback was available. Protocol assertions do not substitute for visual review or physical display validation.

The local containers share the WSL2 kernel listed below; CI uses its Linux runner's kernel. All distributions in one run share that host kernel and use Mesa lavapipe software Vulkan. Annotation persistence uses explicitly seeded SQLite fixtures, and the previous-installation scenario uses ownership fixtures rather than an executed historical binary.

### Six-service GitHub Actions provenance

The final [GitHub Actions run 38158919754](https://github.com/Alexrb11/rastera/actions/runs/38158919754) passed verification and all six runtime services, checking out harness commit `08857c54cbbb9738806ff610036b97700003fc35` and testing the rebuilt archive above. Its six downloaded report archives matched their GitHub artifact digests and confirmed 66/66 checks, matching application source/archive metadata and all nine native Wayland trace hashes. That revision selects GL for the interactive nested compositor; automated Wayland still uses headless pixman. The workflow does not run the manual browser-desktop checks.

The preceding expanded [run 38158383951](https://github.com/Alexrb11/rastera/actions/runs/38158383951) also passed verification and all six services, using harness commit `267a8180f465f6641d8b0fe7ca124139f74b630e`. Its six downloaded report artifacts confirmed 11 passing checks each and matching archive/source metadata. Both expanded runs are distinct from the historical three-service X11 result.

### Rebuilt Wayland browser-desktop review

The separate Ubuntu 24.04 review passed **9/9 manual checks** using Weston 13 with its X11 backend nested inside Xvfb for browser viewing. Rastera is a native Wayland client without `DISPLAY`; the outer X11 window belongs to Weston, and Xwayland is disabled. The same `cf56c2e` binary was used throughout. The first eight checks were observed with the initial pixman nested compositor; the ninth covers the final GL/llvmpipe compositor at harness revision `08857c54cbbb9738806ff610036b97700003fc35` and an actual container restart. Real interaction confirmed:

- The favorite and `wayland` tag saved during the earlier practice session remained visible after opening the rebuilt application, with matching SQLite values.
- Window maximization and restoration.
- Opening the GTK portal chooser, cancelling it, reopening it and selecting a PNG; the viewer decoded the 1254 × 1254 image.
- Fit view and the 100% zoom control, with real screenshots of the viewer.
- Exporting three selected images as PNG at 512 × 512. The new journal recorded `finished` and three `published` items; decoding verified the outputs, and existing JPEG exports from the earlier practice session retained their hashes.
- Library updates during practice-file creation, modification to 900 × 600, renaming and removal; the grid returned from four images to the three originals.
- Adding the `tested` tag through the UI and searching for it, with one of three images displayed.
- Closing Rastera through its own window decoration with application exit code 0 while the nested desktop remained available.

The final GL session retained the favorite and both `wayland`/`tested` tags in the UI and SQLite. Rastera closed through its own window control with exit code 0. An actual Docker restart then launched a new native Wayland window, retained the annotations, and showed the `tested` search result as one of three images. The three original practice PNG hashes remained unchanged. Both GL client logs passed the same native-surface/buffer validation used by the automated driver.

| Final nested GL observation | Recorded interval | Result |
| --- | --- | --- |
| First desktop launch | 88.12 seconds | Running healthy; normal Rastera close with exit code 0 |
| Launch after actual Docker restart | At least 31.77 seconds | Running healthy; annotations and original hashes retained |

These are bounded observations, not an endurance test or a guarantee of indefinite compositor stability. The manual checks are separate from the automatic matrix and its seeded SQLite annotations. Batch cancellation, WebP export, HiDPI and accessibility were not exercised in this session. The evidence records are `manual-ui-results.json`, `gl-first-result.json`, `gl-second-result.json` and `manual-environment-gl-after-restart.json`, with real screenshots and SHA-256 values for their evidence files.

Earlier attempts exposed two harness problems: a stale X server lock after container restart, and a nested Weston pixman process exiting with code `-11` (`SIGSEGV`). Boot-specific `/tmp` and `/tmp/rastera-runtime` tmpfs mounts prevent old display locks from surviving restart. The interactive Weston 13 compositor now uses `--renderer=gl` with `LIBGL_ALWAYS_SOFTWARE=1` and `GALLIUM_DRIVER=llvmpipe`; `weston.log` confirms the llvmpipe renderer. The internal cause of the pixman SIGSEGV remains unknown. No automatic retry hides compositor failures.

The harness also waits for Openbox's live EWMH window and advertised fullscreen support before starting nested Weston, and records a failed service's exit code and signal. This readiness gate does not establish the cause of the later SIGSEGV. Failed-attempt reports are preserved separately from the final successful GL observations. Automated headless pixman remains unchanged and passed on all three distributions.

## Historical X11 validation: original archive

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

The original three-service [GitHub Actions run 38154733379](https://github.com/Alexrb11/rastera/actions/runs/38154733379) also passed. It checked out harness commit `354a8a8b304b45b6543ae9672d04fb1a384f5fa8` and tested the archive above. This is historical X11 evidence; it is not a result for the expanded Wayland matrix or a newer archive with the same version number.

## Requirements

Use Docker Desktop with its Linux container engine running, Docker Compose v2, and PowerShell for the Windows runner. Docker Desktop's [Windows setup guide](https://docs.docker.com/desktop/setup/install/windows-install/) describes the supported Linux backends and host requirements. From the repository root, check both the client and the engine:

```powershell
docker compose version
docker info --format '{{.OSType}}'
```

The second command must succeed and print `linux`. An installed Docker CLI does not mean the engine is ready. Complete Docker Desktop's first-run setup and wait for the engine before starting tests. Building the test images requires internet access to download the distribution image and runtime packages.

Obtain `rastera-0.1.0-alpha.3-linux-x64.tar.gz` and its matching `.sha256` sidecar from the same release or Actions artifact. Keep them together. The archive must contain a normal Linux build with `capture=false`; a Windows ZIP or a development capture binary is unsuitable. The runner verifies the archive rather than compiling Rastera inside the containers.

For the current Wayland checks, use an archive built from `cf56c2e` or a later revision containing the application-id fix. The driver requires the native client to send `io.github.Alexrb11.rastera` as its application ID. The older archive recorded above sends a different ID and will intentionally fail that assertion. Distinguish alpha.3 builds by source commit and SHA-256, not by filename alone.

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

Select the native Wayland matrix with `-DisplayBackend wayland`:

```powershell
.\scripts\test-linux-containers.ps1 `
  -Archive ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz" `
  -ReportsDirectory ".\work\linux-container-reports" `
  -DisplayBackend wayland
```

The runner maps the same `-Service` names to their `-wayland` variants. Running the default X11 command and this Wayland command produces the complete six-service matrix, with separate report subfolders. Set `-DisplayBackend x11` explicitly when needed; it is the default. `-Service ubuntu2404 -DisplayBackend wayland` runs only `ubuntu2404-wayland`.

In automated Wayland mode, Weston uses its **headless backend with pixman software rendering**. Rastera and the driver have `DISPLAY` unset, connect through `WAYLAND_DISPLAY=rastera-wayland` in the compositor's `XDG_RUNTIME_DIR`, and cannot fall back to X11. No Xvfb or Xwayland is started. Weston 9 uses `--use-pixman`; newer Weston uses `--renderer=pixman` when its help advertises that option. Socket ownership and a successful `wayland-info` or older `weston-info` connection establish compositor readiness; the separate Rastera client trace establishes the application's connection.

Only the repository's `scripts` directory is mounted at `/workspace/scripts` read-only. The archive directory is mounted at `/release` read-only, and each service's report subfolder at `/reports` with write access. Images and user profiles used by the driver are temporary copies inside the container. The automated containers have networking disabled at runtime and are limited to two CPUs and 2 GiB of memory. Docker's [bind mount documentation](https://docs.docker.com/engine/storage/bind-mounts/) explains the mount options.

Use `-ExpectedCommit` with a full lowercase Git SHA to require a particular archive source revision. Without that option, the container entrypoint reads the archive's declared source commit and passes it to the verifier and driver; this checks consistency with package metadata. Supply the option when checking against an independently known revision. Desktop mode also checks native package metadata and applies an exact revision check when the option is supplied. The current driver scenario targets `0.1.0-alpha.3`; review its version fixtures before testing a different release. Direct invocation of `test-linux-alpha.py` without `--expected-commit` retains the recorded alpha.3 source commit as its default.

[test-linux-alpha.py](../scripts/docker/test-linux-alpha.py) checks:

- Archive checksum, safe extraction, build metadata, architecture and native dependencies through the package verifier.
- An unprivileged runtime with no `rustc` or `cargo` on PATH.
- Normal GUI startup on the selected backend, opening a temporary folder with one practice JPEG, decoding it and recording its dimensions in SQLite.
- In Wayland GUI runs, a Rastera-owned client trace showing registry exchange, creation of `wl_surface`, `xdg_surface` and `xdg_toplevel`, the expected application ID, matching configure/acknowledge serials, and a non-null buffer attached and committed to that same application surface. `WAYLAND_DEBUG=client` supplies the trace; protocol evidence is checked for portable, installed and replacement launches.
- The real per-user installer in a temporary HOME containing spaces, launcher startup, refusal to reinstall the same version, and replacement of an explicitly declared earlier-installation fixture.
- Persistence of a favorite and tag seeded while Rastera is closed, across portable and installed application launches.
- MIME registration without replacing existing default handlers, and unchanged hashes of the copied practice images.

The earlier-installation fixture checks ownership and replacement behavior; it is not an upgrade test against a historical Rastera binary. The annotation fixture checks persistence, rather than editing favorites or tags through the UI.

## Run the matrix in GitHub Actions

The manual [Linux release containers workflow](../.github/workflows/linux-containers.yml) runs all six runtime services without a Rust toolchain in the images. In **Actions → Linux release containers → Run workflow**, supply `package_run_id`, the ID of a completed, successful **Desktop alpha** packaging run in this repository containing `linux-x64-alpha` and the application-id fix required above. The workflow checks that run's repository, workflow and outcome before downloading its artifact, and verifies the package against that run's source commit.

Each service uploads a `linux-container-<service>` report artifact, including on test failure when reports exist. This workflow does not start the interactive noVNC desktop. Its outcome is separate from the local matrix results recorded above.

## Inspect reports

Each tested service writes `results.json` with its outcome, archive SHA-256, source commit, environment and individual checks. It also writes `native-verification.json` and logs for native verification, installation, refused reinstallation and replacement. GUI logs are `portable-gui.log`, `installed-gui.log` and `upgrade-gui.log`; a failed driver run adds `failure.log`. The host runner records `image.json` with the built image ID and Compose-file hash. `environment.json`, `linux-check.log`, `xvfb.log` and `openbox.log` describe the container's display and execution setup.

Headless Wayland writes `weston.log`, `wayland-readiness.log` and `portal-environment.log` instead of Xvfb/Openbox startup logs. `environment.json` records the display backend, compositor version, selected backend and renderer, socket, readiness tool and disabled Xwayland. X11 and nested Wayland also record `openbox-readiness.log`; a failed required service adds its PID, exit code and signal to the environment report. For each Wayland GUI launch, `results.json` includes the client's observed protocol messages, surface/buffer IDs and GUI-log SHA-256. An advertised compositor interface in the readiness log alone is not proof that Rastera created and committed its own native window.

A passing process exit and passing recorded assertions establish the checks listed above. Keep the JSON and relevant logs when reporting a failure, together with the host's Docker/Compose versions. Check the recorded source commit and archive hash before comparing different runs. Do not include private images or user profiles in an issue.

## Open the browser desktop

The optional `desktop` Compose profile uses Ubuntu 24.04 and the distribution's noVNC/websockify packages. Start its X11 service separately from the matrix:

```powershell
.\scripts\test-linux-containers.ps1 `
  -Archive ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz" `
  -ReportsDirectory ".\work\linux-desktop-reports" `
  -Desktop
```

Open [the local desktop](http://127.0.0.1:6080/vnc.html?autoconnect=true&resize=scale) in a browser. The port is bound to loopback and the local viewer has no password. The profile stays running for manual checks; reports for this mode are stored under the report directory's `desktop` subfolder. Compose [profiles](https://docs.docker.com/compose/how-tos/profiles/) keep this interactive service separate from the default matrix.

For a visible native Wayland application, start the separate service:

```powershell
.\scripts\test-linux-containers.ps1 `
  -Archive ".\work\releases\rastera-0.1.0-alpha.3-linux-x64.tar.gz" `
  -ReportsDirectory ".\work\linux-desktop-reports" `
  -DisplayBackend wayland `
  -Desktop
```

Open [the Wayland browser desktop](http://127.0.0.1:6081/vnc.html?autoconnect=true&resize=scale). Reports are stored in `desktop-wayland`. For this visual review, **Weston 13 runs its X11 backend inside Xvfb with GL rendered by Mesa llvmpipe**, and VNC shows the compositor's outer X11 window. Rastera itself has no `DISPLAY` and connects only to Weston's Wayland socket; Xwayland is disabled. This software-rendered nested arrangement differs from the automated headless pixman backend and from a physical Wayland desktop. D-Bus activation variables and `GDK_BACKEND=wayland` direct GTK/portal processes to the same compositor.

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

The screenshot appears on the host as `desktop/desktop-review.png`. For Wayland, substitute `desktop-wayland` for the service name in `logs`, `exec` and `restart`; its screenshot appears in `desktop-wayland/desktop-review.png`. The `import -display :99` command captures the nested compositor's outer X11 desktop, while Rastera still uses its native Wayland connection. In the nested desktop, close Rastera through its own window decoration: Openbox's outer Alt+F4 binding can close Weston instead of the application.

Desktop mode also records `package-verification.json`, `package-verification.log`, `environment.json`, `rastera.log`, `x11vnc.log` and `websockify.log`; Wayland adds compositor/readiness/activation logs and client protocol in `rastera.log`. If Rastera is closed, the browser desktop remains running. To restart its normal app with the same catalog, or stop the environment:

```powershell
docker compose -f .\scripts\docker\compose.yaml --profile desktop restart desktop
docker compose -f .\scripts\docker\compose.yaml --profile desktop down
```

The `desktop-home` and `desktop-wayland-home` named volumes keep the X11 and Wayland test profiles separate. Each retains practice copies, catalog and exports across container restarts and normal `down`. Display sockets and locks live in boot-specific tmpfs mounts at `/tmp` and `/tmp/rastera-runtime`; they are recreated on restart and do not share the persistent HOME. Logs append across desktop starts, while `environment.json` describes the latest run. Save it under another filename before restarting when preserving a failed attempt.

To start with fresh test profiles, use `down --volumes`; this removes both declared desktop volumes and their test data. Host report files remain in the chosen report directory.

## What this environment establishes

The three distribution images exercise different Linux user-space libraries while sharing Docker Desktop's Linux kernel. Xvfb supplies the X11 display; headless Weston supplies the automated native Wayland session. Interactive Wayland instead nests Weston inside Xvfb for VNC viewing. Mesa supplies software Vulkan to Rastera in both cases. Passing results establish only the recorded runtime combinations and assertions, with no Rust development environment present.

They do not establish compatibility with macOS, physical GPUs or their drivers, full GNOME/KDE Wayland sessions, every desktop portal, HiDPI configurations, input methods (IME), accessibility tools, network filesystems or clean installations on independent Linux kernels. A Weston client trace demonstrates native Wayland protocol use, not general compatibility with every compositor. Test those environments on real target desktops before declaring broader support. The [Linux installation guide](install-linux.md) lists the application's runtime requirements and current experimental scope.
