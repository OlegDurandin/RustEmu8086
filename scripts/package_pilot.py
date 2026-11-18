#!/usr/bin/env python3
"""Package a local, unpublished pilot. Does not upload, deploy or include browser/user data."""
import hashlib
import json
from pathlib import Path
import zipfile

root = Path(__file__).resolve().parents[1]
dist = root / 'crates/rustemu-web/dist'
if not (dist / 'index.html').is_file() or not list(dist.glob('*_bg.wasm')):
    raise SystemExit('Build the web release first')
out = root / 'output/pilot'
out.mkdir(parents=True, exist_ok=True)
archive = out / 'rustemu8086-pilot-readiness-v1.zip'
files = {}
for folder, prefix in [(dist, 'web'), (root / 'examples/learn', 'exercises'),
                       (root / 'examples/projects', 'roundtrip-projects'),
                       (root / 'tests/learning-solutions', 'reference-solutions'),
                       (root / 'reports/pilot-readiness', 'reports')]:
    for path in sorted(folder.rglob('*')):
        if path.is_file():
            files[str(Path(prefix) / path.relative_to(folder))] = path.read_bytes()
files['LICENSE'] = (root / 'LICENSE').read_bytes()
files['START-HERE.txt'] = b'''PRIVATE PILOT CANDIDATE - NOT VALIDATED ON IPAD
Read reports/GO-NO-GO.md and reports/KNOWN-LIMITATIONS.md before inviting users.
Run a local static server: python3 -m http.server 8080 --directory web
Open http://127.0.0.1:8080/?ui=tablet or /?ui=desktop
Use only URLs/tools allowed in your environment. No account or backend needed.
Reference solutions are deliberately included for the pilot coordinator.
Keep the previous complete bundle separately for rollback; never clear browser storage.
No progress, personal drive C:, credentials or user source drafts are included.
'''
manifest = {name: hashlib.sha256(data).hexdigest() for name, data in files.items()}
files['SHA256-MANIFEST.json'] = (json.dumps(manifest, indent=2) + '\n').encode()
with zipfile.ZipFile(archive, 'w', zipfile.ZIP_DEFLATED) as z:
    for name, data in files.items():
        z.writestr(name, data)
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    for name, digest in manifest.items():
        assert hashlib.sha256(z.read(name)).hexdigest() == digest
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_suffix('.zip.sha256').write_text(digest + '  ' + archive.name + '\n')
print(archive, digest, len(files), 'files')
