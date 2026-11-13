"""Fake-root tests for install-ae-plugin.ps1 / restore-ae-plugin.ps1 (Windows).

Never touches the real MediaCore or AE folders: every run uses -DestinationRoot
and -BackupRoot inside a temporary directory, -SkipBuild with fake artifacts,
and a stubbed Get-Process for the "AE is running" case.

    python -m unittest discover -s scripts -p test_install_ae_plugin.py
"""
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPTS = Path(__file__).resolve().parent
SUBFOLDER = 'onmk-aeplugin-rust-template'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


@unittest.skipUnless(sys.platform == 'win32', 'PowerShell installer is Windows-only')
class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix='ae-install-test-'))
        self.dist = self.tmp / 'dist'
        self.root = self.tmp / 'fake-root'
        self.backup = self.tmp / 'backups'
        self.media = self.root / 'MediaCore'
        self.ae_plugins = self.root / 'Adobe After Effects 2099' / 'Support Files' / 'Plug-ins'
        for d in (self.dist, self.media, self.ae_plugins / 'Effects'):
            d.mkdir(parents=True)
        self.write_artifact('AeMinimal.aex', b'ONMK AeMinimal', b'v1')
        self.write_artifact('AeGpuLab.aex', b'ONMK AeGpuLab', b'v1')
        # Unrelated user content that must never change.
        (self.media / 'Other.aex').write_bytes(b'MZ other vendor ONMK Something')
        (self.media / 'notes.txt').write_bytes(b'user content')
        (self.ae_plugins / 'Effects' / 'Builtin.aex').write_bytes(b'MZ builtin')
        self.baseline = self.snapshot()

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def write_artifact(self, name, match, version):
        (self.dist / name).write_bytes(b'MZ\x00fake aex ' + match + b' ' + version)

    def snapshot(self):
        return {str(p.relative_to(self.root)): sha(p) for p in self.root.rglob('*') if p.is_file()}

    def target(self, name):
        return self.media / SUBFOLDER / name

    def sessions(self):
        return sorted(p for p in self.backup.glob('*') if (p / 'journal.json').is_file())

    def run_ps(self, script, *extra, ok=True, prefix=None):
        args = ['-DestinationRoot', str(self.root), '-BackupRoot', str(self.backup), *extra]
        if script == 'install-ae-plugin.ps1':
            args += ['-SkipBuild', '-OutDir', str(self.dist)]
        path = SCRIPTS / script
        if prefix:
            quoted = ' '.join("'" + a.replace("'", "''") + "'" if not a.startswith('-') else a for a in args)
            cmd = ['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-Command',
                   f"{prefix}; & '{path}' {quoted}; exit $LASTEXITCODE"]
        else:
            cmd = ['powershell', '-NoProfile', '-ExecutionPolicy', 'Bypass', '-File', str(path), *args]
        result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=180)
        out = result.stdout.decode(errors='replace')
        self.assertEqual(result.returncode == 0, ok, f'{script} {extra}: exit {result.returncode}\n{out[-3000:]}')
        return out

    def install(self, *plugins, ok=True, prefix=None):
        return self.run_ps('install-ae-plugin.ps1', '-Plugin', ','.join(plugins), '-Install', ok=ok, prefix=prefix)

    def restore(self, *extra, ok=True):
        return self.run_ps('restore-ae-plugin.ps1', *extra, ok=ok)

    def test_refuses_while_after_effects_runs(self):
        stub = "function global:Get-Process { param($Name, $ErrorAction); [pscustomobject]@{ ProcessName = 'AfterFX' } }"
        out = self.install('minimal', ok=False, prefix=stub)
        self.assertIn('AfterFX', out)
        self.assertEqual(self.snapshot(), self.baseline)
        self.assertFalse(self.backup.exists())

    def test_refuses_stale_artifact_without_match_name(self):
        (self.dist / 'AeMinimal.aex').write_bytes(b'MZ something else')
        self.assertIn('does not contain match name', self.install('minimal', ok=False))
        self.assertEqual(self.snapshot(), self.baseline)

    def test_refuses_duplicate_match_names_ascii_and_utf16(self):
        cases = [(self.ae_plugins / 'Effects' / 'Old.aex', 'ascii'),
                 (self.media / 'AeMinimal.aex', 'utf-16-le'),
                 (self.media / 'Nested' / 'x.aex', 'utf-16-be')]
        for path, encoding in cases:
            with self.subTest(path=path.name, encoding=encoding):
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'MZ' + 'ONMK AeMinimal'.encode(encoding))
                out = self.install('minimal', ok=False)
                self.assertIn('duplicate', out)
                path.unlink()
                self.assertEqual(self.snapshot(), self.baseline)
                self.assertFalse(self.backup.exists())

    def test_install_upgrade_restore_cycle(self):
        # Fresh install of two plug-ins.
        self.install('minimal', 'lab')
        v1 = sha(self.dist / 'AeMinimal.aex')
        self.assertEqual(sha(self.target('AeMinimal.aex')), v1)
        self.assertEqual(sha(self.target('AeGpuLab.aex')), sha(self.dist / 'AeGpuLab.aex'))
        installed = self.snapshot()
        self.assertEqual(len(self.sessions()), 1)
        journal = json.loads((self.sessions()[0] / 'journal.json').read_text(encoding='utf-8-sig'))
        self.assertEqual({f['existed'] for f in journal['files']}, {False})

        # Idempotent: nothing changes, no new session.
        self.install('minimal', 'lab')
        self.assertEqual(self.snapshot(), installed)
        self.assertEqual(len(self.sessions()), 1)

        # Upgrade one plug-in: the overwritten file is backed up and verified.
        self.write_artifact('AeMinimal.aex', b'ONMK AeMinimal', b'v2')
        self.install('minimal')
        self.assertEqual(sha(self.target('AeMinimal.aex')), sha(self.dist / 'AeMinimal.aex'))
        self.assertEqual(len(self.sessions()), 2)
        journal = json.loads((self.sessions()[1] / 'journal.json').read_text(encoding='utf-8-sig'))
        [entry] = journal['files']
        self.assertTrue(entry['existed'])
        self.assertEqual(entry['original_sha256'], v1)
        self.assertEqual(sha(Path(entry['backup'])), v1)

        # Restore newest session -> back to v1; twice is harmless.
        self.restore()
        self.assertEqual(self.snapshot(), installed)
        self.restore()
        self.assertEqual(self.snapshot(), installed)

        # Restore the first session -> added files removed, user files intact.
        self.restore('-Session', self.sessions()[0].name)
        self.assertEqual(self.snapshot(), {**self.baseline})

    def test_restore_refuses_modified_target_and_corrupt_backup(self):
        self.install('minimal')
        self.write_artifact('AeMinimal.aex', b'ONMK AeMinimal', b'v2')
        self.install('minimal')
        upgraded = self.snapshot()

        # A user edit after install stops the whole restore.
        target = self.target('AeMinimal.aex')
        original = target.read_bytes()
        target.write_bytes(b'edited by user')
        edited = self.snapshot()
        self.assertIn('changed since install', self.restore(ok=False))
        self.assertEqual(self.snapshot(), edited)
        target.write_bytes(original)

        # A corrupt backup is detected before anything changes.
        journal = json.loads((self.sessions()[-1] / 'journal.json').read_text(encoding='utf-8-sig'))
        backup = Path(journal['files'][0]['backup'])
        good = backup.read_bytes()
        backup.write_bytes(b'corrupt')
        self.assertIn('corrupt', self.restore(ok=False))
        self.assertEqual(self.snapshot(), upgraded)
        backup.write_bytes(good)
        self.restore()

    def test_backup_root_inside_destination_is_refused(self):
        self.backup = self.media / 'backups'
        self.assertIn('Backup root must be outside', self.install('minimal', ok=False))
        self.assertEqual(self.snapshot(), self.baseline)


if __name__ == '__main__':
    unittest.main()
