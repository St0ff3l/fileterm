$ErrorActionPreference = 'Stop'

if (-not (Get-Command git -CommandType Application -ErrorAction SilentlyContinue)) {
  Write-Error 'Git for Windows is required to run the CSS contract check.'
  exit 1
}
if (-not (Get-Command rg -CommandType Application -ErrorAction SilentlyContinue)) {
  Write-Error 'ripgrep (rg) must be available on PATH to run the CSS contract check.'
  exit 1
}

$gitExecPath = & git --exec-path
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
$gitRoot = Split-Path (Split-Path (Split-Path $gitExecPath -Parent) -Parent) -Parent
$bash = Join-Path $gitRoot 'bin/bash.exe'
if (-not (Test-Path -LiteralPath $bash -PathType Leaf)) {
  Write-Error 'Git Bash was not found. Install Git for Windows to run the CSS contract check.'
  exit 1
}

& $bash (Join-Path $PSScriptRoot 'check-css-contract.sh')
exit $LASTEXITCODE
