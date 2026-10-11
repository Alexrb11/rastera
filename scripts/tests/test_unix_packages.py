"""Regression tests for release archive traversal, modes and native metadata."""
import importlib.util
import io
import json
import os
import plistlib
import stat
import struct
import sys
import tarfile
import tempfile
import unittest
from unittest import mock
from pathlib import Path
from zipfile import ZipFile, ZipInfo

SCRIPTS = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(SCRIPTS))
from package_unix_support import architecture, bundle_plist, check_bundle, relative_path


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


verifier = load("unix_verifier", SCRIPTS / "verify-unix-package.py")
installer = load("linux_installer", SCRIPTS.parent / "resources/linux/install-user.py")


class PackageSafety(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.extracted = self.root / "extracted"
        self.extracted.mkdir()

    def tearDown(self):
        self.temporary.cleanup()

    def zip(self, entries):
        archive = self.root / "release.zip"
        with ZipFile(archive, "w") as stream:
            for name, mode in entries:
                item = ZipInfo(name)
                item.create_system = 3
                item.external_attr = mode << 16
                stream.writestr(item, b"test")
        return archive

    def assert_rejected(self, archive):
        with self.assertRaises(ValueError):
            verifier.safe_extract(archive, self.extracted)
        self.assertEqual(list(self.extracted.iterdir()), [])

    def test_zip_traversal_is_rejected_before_any_file_is_written(self):
        archive = self.zip([("package/good", stat.S_IFREG | 0o644), ("package/../../escape", stat.S_IFREG | 0o644)])
        self.assert_rejected(archive)
        self.assertFalse((self.root / "escape").exists())

    def test_zip_symlink_and_world_writable_files_are_rejected(self):
        for mode in (stat.S_IFLNK | 0o755, stat.S_IFREG | 0o666, stat.S_IFREG | 0o4755):
            with self.subTest(mode=mode):
                self.assert_rejected(self.zip([("package/link", mode)]))

    def test_file_parent_collisions_are_rejected(self):
        self.assert_rejected(self.zip([("package/parent", stat.S_IFREG | 0o644), ("package/parent/file", stat.S_IFREG | 0o644)]))

    def test_multiple_package_roots_are_rejected(self):
        self.assert_rejected(self.zip([("a/file", stat.S_IFREG | 0o644), ("b/file", stat.S_IFREG | 0o644)]))

    def test_tar_hard_links_are_rejected(self):
        archive = self.root / "release.tar.gz"
        with tarfile.open(archive, "w:gz") as stream:
            item = tarfile.TarInfo("package/link")
            item.type = tarfile.LNKTYPE
            item.linkname = "../../outside"
            item.mode = 0o644
            stream.addfile(item)
        self.assert_rejected(archive)

    def test_tar_normal_file_and_executable_survive_extraction(self):
        archive = self.root / "release.tar.gz"
        with tarfile.open(archive, "w:gz") as stream:
            for name, mode in (("package/README", 0o644), ("package/bin/rastera", 0o755)):
                item = tarfile.TarInfo(name)
                item.mode = mode
                item.size = 4
                stream.addfile(item, io.BytesIO(b"test"))
        package = verifier.safe_extract(archive, self.extracted)
        self.assertEqual((package / "README").read_bytes(), b"test")
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE((package / "bin/rastera").stat().st_mode), 0o755)
            self.assertEqual(stat.S_IMODE((package / "README").stat().st_mode), 0o644)

    def test_zip_normal_file_and_executable_survive_extraction(self):
        package = verifier.safe_extract(self.zip([("package/README", stat.S_IFREG | 0o644), ("package/bin/rastera", stat.S_IFREG | 0o755)]), self.extracted)
        self.assertEqual((package / "bin/rastera").read_bytes(), b"test")
        if os.name != "nt":
            self.assertEqual(stat.S_IMODE((package / "bin/rastera").stat().st_mode), 0o755)

    def test_ambiguous_paths_are_rejected(self):
        for path in ("../out", "/absolute", "C:/windows", "a\\b", "a//b", "a/./b", "a/../b", ""):
            with self.subTest(path=path), self.assertRaises(ValueError):
                relative_path(path)

    def test_elf_architecture_is_checked(self):
        executable = self.root / "rastera"
        header = bytearray(32)
        header[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<HH", header, 16, 3, 62)
        executable.write_bytes(header)
        self.assertEqual(architecture(executable, "x86_64-unknown-linux-gnu"), "ELF64 x86_64")
        struct.pack_into("<H", header, 18, 183)
        executable.write_bytes(header)
        with self.assertRaises(ValueError):
            architecture(executable, "x86_64-unknown-linux-gnu")

    def test_macho_architecture_is_checked(self):
        executable = self.root / "rastera"
        executable.write_bytes(struct.pack("<IIII", 0xFEEDFACF, 0x0100000C, 0, 2) + bytes(16))
        self.assertEqual(architecture(executable, "aarch64-apple-darwin"), "Mach-O arm64")
        with self.assertRaises(ValueError):
            architecture(executable, "x86_64-apple-darwin")

    def test_bundle_versions_icon_and_no_document_handlers(self):
        contents = self.root / "Rastera.app/Contents"
        resources = contents / "Resources"
        resources.mkdir(parents=True)
        info = bundle_plist("0.1.0-alpha.3")
        self.assertEqual(info["CFBundleShortVersionString"], "0.1.0")
        self.assertEqual(info["CFBundleVersion"], "0.1.0")
        self.assertEqual(info["LSMinimumSystemVersion"], "13.0")
        self.assertNotIn("CFBundleDocumentTypes", info)
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump(info, stream)
        (resources / "rastera.icns").write_bytes(b"icon")
        self.assertEqual(check_bundle(self.root, "0.1.0-alpha.3"), self.root / "Rastera.app")
        info["LSMinimumSystemVersion"] = "15.0"
        with (contents / "Info.plist").open("wb") as stream:
            plistlib.dump(info, stream)
        with self.assertRaises(ValueError):
            check_bundle(self.root, "0.1.0-alpha.3")

    def test_desktop_exec_escapes_paths_including_percent_field_codes(self):
        self.assertEqual(installer.desktop_argument("/home/a space/app"), '"/home/a space/app"')
        self.assertEqual(installer.desktop_argument("/home/100%/app"), '"/home/100%%/app"')
        self.assertIn('\\\\"', installer.desktop_argument('/home/a"b/app'))
        with self.assertRaises(ValueError):
            installer.desktop_argument("/home/a\nInjected=true")

    def fake_installer_source(self, version="0.1.0-alpha.3"):
        source = self.root / "package"
        (source / "bin").mkdir(parents=True, exist_ok=True)
        (source / "bin/rastera").write_text("sample binary")
        (source / "assets/branding").mkdir(parents=True, exist_ok=True)
        (source / "assets/branding/rastera-256.png").write_bytes(b"icon")
        (source / "share/applications").mkdir(parents=True, exist_ok=True)
        (source / "share/applications/rastera.desktop").write_text("[Desktop Entry]\nType=Application\nExec=rastera %f\n")
        (source / "BUILD-INFO.json").write_text(json.dumps({"version": version, "capture": False, "target": "x86_64-unknown-linux-gnu"}))
        return source

    def test_install_and_upgrade_are_user_local_and_preserve_default_handlers(self):
        source = self.fake_installer_source()
        home = self.root / "user with spaces"
        with mock.patch.object(installer, "__file__", str(source / "install-user.py")), mock.patch.object(installer.Path, "home", return_value=home), mock.patch.object(installer.shutil, "which", return_value=None):
            installer.install()
            desktop = home / ".local/share/applications/io.github.Alexrb11.rastera.desktop"
            self.assertNotIn("MimeType=", desktop.read_text())
            self.assertTrue((home / ".local/share/rastera/releases/0.1.0-alpha.3/bin/rastera").is_file())
            self.fake_installer_source("0.1.0-alpha.4")
            with self.assertRaises(FileExistsError):
                installer.install()
            installer.install(register_mime=True, replace=True)
            self.assertIn("MimeType=image/jpeg;image/png;image/webp;", desktop.read_text())
            self.assertIn("0.1.0-alpha.4", (home / ".local/bin/rastera").read_text())
            self.assertTrue((home / ".local/share/rastera/releases/0.1.0-alpha.3").is_dir())
            self.assertFalse((home / ".config/mimeapps.list").exists())

    def test_installer_rejects_versions_which_escape_user_local_installation(self):
        source = self.fake_installer_source("../../outside")
        with mock.patch.object(installer, "__file__", str(source / "install-user.py")), mock.patch.object(installer.Path, "home", return_value=self.root / "home"):
            with self.assertRaises(ValueError):
                installer.install()
        self.assertFalse((self.root / "home/.local").exists())


if __name__ == "__main__":
    unittest.main()
