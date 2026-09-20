"""Isolated installer regression checks; never touches the real Termux HOME."""
import hashlib
import io
import os
from pathlib import Path
import subprocess
import shutil
import tarfile
import tempfile
import unittest

INSTALLER = Path(__file__).with_name('install.sh').resolve()


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        for name, body in {
            'dpkg': 'echo arm',
            'rg': 'exit 0',
            'fd': 'exit 0',
            'pkg': 'exit 99',
            'npm': 'mkdir -p "$3/node_modules/.bin"; touch "$3/node_modules/.bin/pi"; chmod +x "$3/node_modules/.bin/pi"',
        }.items():
            path = self.bin / name
            path.write_text('#!/bin/bash\n' + body + '\n')
            path.chmod(0o700)
        self.env = dict(os.environ, PREFIX=str(self.root), SYNTAXIS_TERMUX_HOME=str(self.root), PATH=f'{self.bin}:{os.environ["PATH"]}')
        self.install = self.root / '.local/share/syntaxis'

    def bundle(self, version='1', abi='armeabi-v7a'):
        path = self.root / f'{version}.tar.gz'
        with tarfile.open(path, 'w:gz') as archive:
            for name, content in {'server': version, 'start.sh': '#!/bin/bash\n', 'pi-version.txt': '1.2.3', 'revision.txt': version, 'abi.txt': abi, 'public/index.html': version}.items():
                data = content.encode()
                info = tarfile.TarInfo(name)
                info.size = len(data)
                archive.addfile(info, io.BytesIO(data))
        Path(str(path) + '.sha256').write_text(hashlib.sha256(path.read_bytes()).hexdigest() + '  bundle\n')
        return path

    def run_install(self, *args, success=True):
        result = subprocess.run(['bash', str(INSTALLER), *map(str, args), '--no-start', '--no-app-launch'], env=self.env, capture_output=True, text=True)
        self.assertEqual(result.returncode == 0, success, result.stdout + result.stderr)
        return result

    def mock_release(self, complete=True, tamper=False):
        assets = self.root / 'release-assets'
        assets.mkdir()
        archive = self.bundle('release')
        name = 'syntaxis-termux-armeabi-v7a.tar.gz'
        shutil.copy(archive, assets / name)
        shutil.copy(str(archive) + '.sha256', assets / (name + '.sha256'))
        installer = assets / 'install-syntaxis.sh'
        shutil.copy(INSTALLER, installer)
        (assets / 'Android-SHA256SUMS').write_text(hashlib.sha256(installer.read_bytes()).hexdigest() + '  install-syntaxis.sh\n')
        if tamper:
            installer.write_text('exit 99\n')
        if not complete:
            (assets / name).unlink()
        curl = self.bin / 'curl'
        curl.write_text(r"""#!/usr/bin/env python3
import os, pathlib, shutil, sys
args = sys.argv[1:]
url = args[-1]
root = pathlib.Path(os.environ['TEST_RELEASE_ASSETS'])
with (root / 'requests').open('a') as log:
    log.write(url + '\n')
if url == 'https://github.com/katawaredev/syntaxis/releases/latest':
    print('https://github.com/katawaredev/syntaxis/releases/tag/v0.13.0', end='')
else:
    assert url.startswith('https://github.com/katawaredev/syntaxis/releases/download/v0.13.0/')
    source = root / url.rsplit('/', 1)[1]
    if not source.is_file():
        sys.exit(22)
    shutil.copy(source, args[args.index('--output') + 1])
""")
        curl.chmod(0o700)
        self.env['TEST_RELEASE_ASSETS'] = str(assets)
        return assets

    def test_latest_download_pins_assets_and_saves_updater(self):
        assets = self.mock_release()
        self.run_install('--latest')
        self.assertTrue((self.install / 'update.sh').is_file())
        self.assertEqual((self.install / 'current/server').read_text(), 'release')
        requests = (assets / 'requests').read_text().splitlines()
        self.assertEqual(len(requests), 5)
        self.assertTrue(all('/download/v0.13.0/' in url for url in requests[1:]))
        # The saved updater can safely replace its own file on a later invocation.
        updated = subprocess.run(['bash', str(self.install / 'update.sh'), '--latest', '--no-start', '--no-app-launch'], env=self.env, capture_output=True, text=True)
        self.assertEqual(updated.returncode, 0, updated.stdout + updated.stderr)

    def test_pinned_release_does_not_resolve_latest(self):
        assets = self.mock_release()
        self.run_install('--release=v0.13.0')
        self.assertNotIn('/latest', (assets / 'requests').read_text())

    def test_incomplete_download_preserves_existing_release(self):
        self.run_install(self.bundle())
        first = os.readlink(self.install / 'current')
        self.mock_release(complete=False)
        result = self.run_install('--latest', success=False)
        self.assertIn('incomplete or unavailable', result.stderr)
        self.assertEqual(first, os.readlink(self.install / 'current'))

    def test_downloaded_installer_checksum_is_checked_before_execution(self):
        self.mock_release(tamper=True)
        result = self.run_install('--latest', success=False)
        self.assertIn('Installer checksum mismatch', result.stderr)
        self.assertFalse((self.install / 'current').exists())

    def test_update_rollback_and_preserved_data(self):
        project = self.root / 'Projects/work/file'
        project.parent.mkdir(parents=True)
        project.write_text('keep me')
        password = self.root / '.config/syntaxis/password.hash'
        password.parent.mkdir(parents=True)
        password.write_text('existing hash')
        self.run_install(self.bundle())
        first = os.readlink(self.install / 'current')
        self.run_install(self.bundle('2'))
        self.assertNotEqual(first, os.readlink(self.install / 'current'))
        self.run_install('--rollback')
        self.assertEqual(first, os.readlink(self.install / 'current'))
        self.assertEqual(project.read_text(), 'keep me')
        self.assertEqual(password.read_text(), 'existing hash')

    def test_checksum_and_architecture_fail_before_activation(self):
        archive = self.bundle(abi='arm64-v8a')
        self.run_install(archive, success=False)
        self.assertFalse((self.install / 'current').exists())
        archive.write_bytes(b'corrupt')
        result = self.run_install(archive, success=False)
        self.assertIn('Checksum mismatch', result.stderr)

    def test_running_backend_blocks_update(self):
        import fcntl
        state = self.root / '.local/state/syntaxis'
        state.mkdir(parents=True)
        with (state / 'server.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            result = self.run_install(self.bundle(), success=False)
        self.assertIn('Stop the running', result.stderr)
        self.assertFalse((self.install / 'current').exists())

    def test_legacy_install_can_roll_back(self):
        (self.install / 'public').mkdir(parents=True)
        (self.install / 'server').write_text('old backend')
        (self.install / 'start.sh').write_text('old launcher')
        (self.install / 'public/index.html').write_text('old UI')
        self.run_install(self.bundle())
        self.run_install('--rollback')
        self.assertEqual((self.install / 'current/server').read_text(), 'old backend')

    def test_traversal_and_links_are_rejected(self):
        for name, link in [('../escape', False), ('public/link', True)]:
            path = self.root / 'unsafe.tar.gz'
            with tarfile.open(path, 'w:gz') as archive:
                info = tarfile.TarInfo(name)
                if link:
                    info.type = tarfile.SYMTYPE
                    info.linkname = '/tmp'
                archive.addfile(info)
            Path(str(path) + '.sha256').write_text(hashlib.sha256(path.read_bytes()).hexdigest() + '  bundle\n')
            self.run_install(path, success=False)
            self.assertFalse((self.install / 'current').exists())

    def test_failed_pi_install_preserves_current(self):
        self.run_install(self.bundle())
        first = os.readlink(self.install / 'current')
        (self.install / 'pi/1.2.3/node_modules/.bin/pi').unlink()
        (self.bin / 'npm').write_text('#!/bin/bash\nexit 1\n')
        self.run_install(self.bundle('2'), success=False)
        self.assertEqual(first, os.readlink(self.install / 'current'))


if __name__ == '__main__':
    unittest.main()
