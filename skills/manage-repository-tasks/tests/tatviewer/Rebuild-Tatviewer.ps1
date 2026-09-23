[CmdletBinding()]
param()

$skillPath = [System.IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..\..'))
$cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
$cargoPath = if ($cargoCommand) {
    $cargoCommand.Source
}
else {
    $candidateRoot = Get-Item -LiteralPath $skillPath
    $candidatePath = $null
    while ($candidateRoot -and -not $candidatePath) {
        $candidate = Join-Path $candidateRoot.FullName '.cargo\bin\cargo.exe'
        if (Test-Path -LiteralPath $candidate -PathType Leaf) {
            $candidatePath = $candidate
        }
        $candidateRoot = $candidateRoot.Parent
    }
    $candidatePath
}

if (-not (Test-Path -LiteralPath $cargoPath -PathType Leaf)) {
    throw "Cargo was not found. Install Rust or add cargo to PATH."
}

Push-Location -LiteralPath $skillPath
try {
    & $cargoPath build --release --locked --bin tat --bin tatviewer
    if ($LASTEXITCODE -ne 0) {
        throw "The tat and tatviewer release build failed with exit code $LASTEXITCODE."
    }

    foreach ($name in @('tat', 'tatviewer')) {
        $releasePath = Join-Path $skillPath "target\release\$name.exe"
        $bundledPath = Join-Path $skillPath "$name.exe"
        if (-not (Test-Path -LiteralPath $releasePath -PathType Leaf)) {
            throw "The release build did not create $releasePath."
        }
        Copy-Item -LiteralPath $releasePath -Destination $bundledPath -Force
    }

    & (Join-Path $PSScriptRoot 'Generate-TatviewerOutput.ps1')
    if ($LASTEXITCODE -ne 0) {
        throw "The tatviewer fixture generator failed with exit code $LASTEXITCODE."
    }
}
finally {
    Pop-Location
}
