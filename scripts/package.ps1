param([string]$EvalRoot)
$ErrorActionPreference = 'Stop'
Push-Location (Join-Path $PSScriptRoot '..')
try {
    node scripts/release.mjs prepare
    if ($LASTEXITCODE -ne 0) { throw 'Version preparation failed' }
    if (!$EvalRoot) {
        python scripts/acquire_inputs.py
        if ($LASTEXITCODE -ne 0) { throw 'Pinned input acquisition failed' }
        $EvalRoot = Join-Path (Get-Location) 'target/public-inputs'
    }
    cargo build --release --locked --bin goodtype-math-recognize
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    python scripts/assemble.py $EvalRoot
    if ($LASTEXITCODE -ne 0) { throw 'Package assembly failed' }
} finally { Pop-Location }
