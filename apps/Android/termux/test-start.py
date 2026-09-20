"""Exercise first-run pairing and restart without real credentials or a device."""
import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class LauncherTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.install = self.root / 'bundle'
        (self.install / 'public').mkdir(parents=True)
        (self.install / 'public/index.html').touch()
        shutil.copy(Path(__file__).with_name('start.sh'), self.install / 'start.sh')
        server = self.install / 'server'
        server.write_text('''#!/usr/bin/env python3
import json, os, sys
if len(sys.argv) > 1:
    assert sys.argv[1] == 'random-password-hash'
    print('generated-hash-fixture')
else:
    fields = ['SYNTAXIS_API_TOKEN', 'SYNTAXIS_PASSWORD_HASH', 'IP', 'PORT', 'SYNTAXIS_PROJECTS_ROOT', 'SYNTAXIS_AUTH_DISABLED']
    with open(os.environ['CAPTURE_ENV'], 'w') as output:
        json.dump({key: os.environ.get(key) for key in fields}, output)
''')
        server.chmod(0o700)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        pi = self.bin / 'pi'
        pi.write_text('#!/bin/bash\nexit 0\n')
        pi.chmod(0o700)
        self.env = dict(os.environ, PREFIX=str(self.root), SYNTAXIS_TERMUX_HOME=str(self.root), CAPTURE_ENV=str(self.root / 'env.json'), PATH=f'{self.bin}:{os.environ["PATH"]}', SYNTAXIS_AUTH_DISABLED='true')

    def run_start(self, *args):
        return subprocess.run(['bash', str(self.install / 'start.sh'), *args], env=self.env, capture_output=True, text=True)

    def test_pairing_creates_no_password_prompt_and_restart_preserves_token(self):
        token = 'a' * 64
        result = self.run_start('--app-token', token)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(token, result.stdout + result.stderr)
        capture = json.loads((self.root / 'env.json').read_text())
        self.assertEqual(capture['SYNTAXIS_API_TOKEN'], token)
        self.assertEqual(capture['SYNTAXIS_PASSWORD_HASH'], 'generated-hash-fixture')
        self.assertEqual(capture['IP'], '127.0.0.1')
        self.assertIsNone(capture['SYNTAXIS_AUTH_DISABLED'])
        self.assertEqual(self.run_start().returncode, 0)
        self.assertEqual((self.root / '.config/syntaxis/android-token').read_text().strip(), token)

    def test_pairing_does_not_replace_a_running_backends_token(self):
        token = self.root / '.config/syntaxis/android-token'
        token.parent.mkdir(parents=True)
        token.write_text('existing-token')
        state = self.root / '.local/state/syntaxis'
        state.mkdir(parents=True)
        with (state / 'server.lock').open('w') as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            self.run_start('--app-token', 'b' * 64)
        self.assertEqual(token.read_text(), 'existing-token')

    def test_invalid_pairing_input_is_rejected(self):
        result = self.run_start('--app-token', 'invalid')
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / '.config/syntaxis/android-token').exists())


if __name__ == '__main__':
    unittest.main()
