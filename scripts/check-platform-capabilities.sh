#!/usr/bin/env bash
# Verify that native-loader triples, published package metadata, and public
# support claims have one source of truth.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

python3 - <<'PY'
import json
import os
from pathlib import Path

root = Path('.')
manifest_path = Path(os.environ.get('MANIFEST_PATH', 'dev/platform-capabilities.json'))
manifest = json.loads((root / manifest_path).read_text())
platforms = manifest['platforms']
triples = {entry['triple']: entry for entry in platforms}
if manifest['schema'] != 1 or not triples:
    raise SystemExit('FAIL platform-capabilities: invalid or empty manifest')
if len(triples) != len(platforms):
    raise SystemExit('FAIL platform-capabilities: duplicate triple')

loader = (root / 'src/ts/src/platform.ts').read_text()
for triple, entry in triples.items():
    if triple not in loader:
        raise SystemExit(f'FAIL platform-capabilities: loader triple {triple} is absent from manifest coverage')
    if entry['status'] not in {'published', 'planned'}:
        raise SystemExit(f'FAIL platform-capabilities: {triple} has invalid status')
    if entry['status'] == 'published':
        package_dir = entry['package_dir']
        if not package_dir:
            raise SystemExit(f'FAIL platform-capabilities: published {triple} has no package directory')
        package = json.loads((root / package_dir / 'package.json').read_text())
        if package['name'] != entry['npm_package'] or triple not in package['main']:
            raise SystemExit(f'FAIL platform-capabilities: package metadata disagrees for {triple}')

published = [entry for entry in platforms if entry['status'] == 'published']
if [entry['triple'] for entry in published] != ['linux-x64-gnu']:
    raise SystemExit('FAIL platform-capabilities: public docs currently support exactly linux-x64-gnu')
for path in ('README.md', 'docs/compatibility/index.md', 'docs/install/python.md', 'docs/install/typescript.md'):
    text = (root / path).read_text()
    if 'Linux x86_64' not in text and 'x86_64-unknown-linux-gnu' not in text:
        raise SystemExit(f'FAIL platform-capabilities: {path} lacks the published-platform boundary')
print(f'ok    platform-capabilities: {len(triples)} loader triples, {len(published)} published')
PY
