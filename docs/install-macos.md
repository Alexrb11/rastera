# Install Rastera on macOS

macOS support is experimental in `0.1.0-alpha.3`. Packages target macOS 13 or later on Apple Silicon and Intel. The build matrix uses macOS 15 runners for both architectures; this does not establish that macOS 13 has been tested. Normal desktop use, permissions, scaling, GPU behavior and accessibility require manual validation. The published Windows alpha remains available while this release is prepared.

## Choose your package

Once this version is published, download a ZIP and its matching `.sha256` from [the official releases](https://github.com/Alexrb11/rastera/releases):

| Your Mac | Archive |
| --- | --- |
| Apple Silicon, such as M1/M2/M3/M4 | `rastera-0.1.0-alpha.3-macos-arm64.zip` |
| Intel | `rastera-0.1.0-alpha.3-macos-x64.zip` |

These are separate native packages, not a universal binary. Open **Apple menu → About This Mac** to check whether it lists a chip or an Intel processor. No Rust installation is required.

Check the checksum in Terminal, in the directory containing the ZIP and its checksum:

```sh
shasum -a 256 -c rastera-0.1.0-alpha.3-macos-arm64.zip.sha256
```

Use the x64 filename instead on Intel. The checksum detects a changed download; it does not authenticate the publisher. Extract the ZIP with Finder and move `Rastera.app` to Applications or another writable location. Open the application itself by double-clicking it.

## Signature and first launch

The bundle has an ad-hoc signature for executable integrity. It has **no Developer ID signature and has not been notarized by Apple**. Gatekeeper may block a downloaded copy. An ad-hoc signature does not establish the developer's identity or guarantee that Gatekeeper will permit it.

If you have verified the download source and checksum and choose to test this alpha, first attempt to open it. Then use **System Settings → Privacy & Security → Open Anyway**, when macOS offers that option, and confirm the application-specific prompt. Follow [Apple's first-launch instructions](https://support.apple.com/en-us/102445). Do not disable Gatekeeper or remove quarantine attributes. If macOS reports malware or a damaged or altered application, do not override that alert; report it and check the download. Managed Macs may restrict this exception.

## Open images and folders

Use **Abrir carpeta…** or **Abrir imagen…** inside Rastera, or drag an image or folder into its window. Cmd+O opens an image and Shift+Cmd+O opens a folder. Cmd+click changes a multiple selection; Cmd+F searches, Cmd+E opens export, and Shift+Cmd+T organizes a selection. The interface and illustrated tutorial are currently in Spanish.

Finder **Open With** and opening an image by double-clicking it are not registered in this alpha. The bundle does not declare file associations: forwarding Finder document events into the existing application remains pending. A command-line argument can open a file or folder when launching the executable directly:

```sh
"/Applications/Rastera.app/Contents/MacOS/rastera" "$HOME/Pictures"
```

macOS may ask for access to protected folders or removable drives. Use the specific folder access that the task needs; Rastera does not require granting Full Disk Access by default.

## Local data and updates

The catalog is `$HOME/Library/Application Support/Rastera/catalog.sqlite`. Thumbnails use `$HOME/Library/Caches/Rastera/thumbnails-v1`. If an earlier source build created a catalog under `$XDG_DATA_HOME/Rastera/catalog.sqlite` or `$HOME/.local/share/Rastera/catalog.sqlite`, Rastera continues using that catalog when the new location has no catalog. It does not move or copy it automatically. The thumbnail cache uses the new Library location.

Replacing `Rastera.app` preserves these files. Cache files are disposable; the catalog stores tags and favorites. To back it up, close Rastera and include SQLite companion files. Export journals remain in the output folder; originals remain in their own folders. HOME must be absolute for application-data paths to resolve.

The application includes licenses, dependency notices and the illustrated tutorial under `Rastera.app/Contents/Resources`. In Finder, **Show Package Contents** exposes that directory; open `docs/tutorial/index.html` in a browser. See [the user guide](usage.md) for selection, export review and recovery.

## Current limits and testing

The scope remains one folder without recursion, up to 10,000 entries, with static 8-bit SDR JPEG, PNG and WebP. RAW, HEIC, animated images and monitor-profile conversion are unsupported. External renames do not transfer tags or favorites. Desktop dialogs, Cmd shortcuts, Retina scaling, folder observation, export collisions, permissions and accessibility remain manual test requirements on Apple Silicon and Intel. Package inspection and `--build-info` do not prove these graphical behaviors.

For a useful report, include version, archive checksum, macOS version, chip/processor, screen scale and reproducible steps. Use practice images you may share. Report security problems through [SECURITY.md](../SECURITY.md).

## Build from source

Use Rust 1.95 or later and Apple's Xcode Command Line Tools. The release target sets a macOS 13 deployment minimum.

```sh
cargo run -p rastera-desktop --locked
cargo run -p rastera-desktop --locked -- "$HOME/Pictures"
```

For package creation and signature verification, see [the release procedure](releasing.md).
