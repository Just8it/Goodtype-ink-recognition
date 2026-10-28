param([Parameter(Mandatory=$true)][string]$EvalRoot)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    cargo build --release --locked --bin goodtype-math-recognize
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    python scripts/assemble.py $EvalRoot
    if ($LASTEXITCODE -ne 0) { throw 'Package assembly failed' }
} finally { Pop-Location }
