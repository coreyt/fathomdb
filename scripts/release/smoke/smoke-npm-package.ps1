param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("fathomdb-npm-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
  Push-Location $work
  npm init -y | Out-Null
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  npm install --silent "fathomdb@$Version"
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  @'
import { Engine } from "fathomdb";
const engine = await Engine.open(process.argv[2]);
await engine.write([{ kind: "doc", body: "{}", sourceId: "smoke:npm-package" }]);
await engine.search("smoke");
await engine.close();
'@ | Set-Content smoke.mjs
  node smoke.mjs (Join-Path $work 'smoke.fdb')
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
  Pop-Location
  Remove-Item -Recurse -Force $work
}
