"""Static package regressions; these are not browser E2E."""
import hashlib
import json
from pathlib import Path
import re
import unittest
import zipfile


class PilotPackageTests(unittest.TestCase):
    def setUp(self):
        self.zip = zipfile.ZipFile(Path(__file__).resolve().parents[1] / 'output/pilot/rustemu8086-pilot-readiness-v1.zip')
        self.addCleanup(self.zip.close)

    def test_root_asset_urls_are_served_from_web_root(self):
        self.assertIn(b'--directory web', self.zip.read('START-HERE.txt'))
        self.assertNotIn(b':8080/web/', self.zip.read('START-HERE.txt'))
        html = self.zip.read('web/index.html').decode()
        for asset in re.findall(r'''["'](/[^"']+\.(?:m?js|wasm|png))["']''', html):
            self.assertIn('web' + asset, self.zip.namelist(), asset)
        self.assertIn('web/rustemu-assembler-worker_loader.js', self.zip.namelist())

    def test_manifest_matches_all_payload_bytes(self):
        for name, digest in json.loads(self.zip.read('SHA256-MANIFEST.json')).items():
            self.assertEqual(hashlib.sha256(self.zip.read(name)).hexdigest(), digest, name)

    def test_twelve_exercises_and_reference_sources(self):
        names = [n for n in self.zip.namelist() if n.startswith('exercises/') and n.endswith('.remu')]
        self.assertEqual(len(names), 12)
        counts = {'8086': 0, 'MIPS32': 0}
        for name in names:
            p = json.loads(self.zip.read(name)); counts[p['architecture']] += 1
            e = p['exercise']; self.assertGreaterEqual(len(e['cases']), 2)
            ext = 'asm' if p['architecture'] == '8086' else 's'
            self.assertIn('reference-solutions/' + e['id'] + '.' + ext, self.zip.namelist())
        self.assertEqual(counts, {'8086': 6, 'MIPS32': 6})


if __name__ == '__main__':
    unittest.main()
