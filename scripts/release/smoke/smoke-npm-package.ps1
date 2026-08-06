param([Parameter(Mandatory = $true)][string]$Version)
$ErrorActionPreference = 'Stop'
$work = Join-Path ([System.IO.Path]::GetTempPath()) ("fathomdb-npm-" + [guid]::NewGuid())
New-Item -ItemType Directory -Path $work | Out-Null
try {
  Push-Location $work
  npm init -y | Out-Null
  npm install --silent "fathomdb@$Version"
  @'
import { Engine } from "fathomdb";
const engine = await Engine.open(process.argv[2]);
await engine.write([{ kind: "doc", body: "{}", sourceId: "smoke:npm-package" }]);
await engine.search("smoke");
await engine.close();
'@ | Set-Content smoke.mjs
  node smoke.mjs (Join-Path $work 'smoke.fdb')
} finally {
  Pop-Location
  Remove-Item -Recurse -Force $work
}
