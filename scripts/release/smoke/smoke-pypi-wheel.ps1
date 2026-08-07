param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("fathomdb-pypi-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
  python -m venv "$work/venv"
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  $python = Join-Path $work 'venv/Scripts/python.exe'
  & $python -m pip install --quiet --upgrade pip
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  & $python -m pip install --quiet "fathomdb==$Version"
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  $db = Join-Path $work 'smoke.fdb'
  @'
import sys
from fathomdb import Engine
engine = Engine.open(sys.argv[1])
engine.write([{"kind": "doc", "body": "{}", "source_id": "smoke:pypi-wheel"}])
engine.search("smoke")
engine.close()
'@ | & $python - $db
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally { Remove-Item -Recurse -Force $work }
