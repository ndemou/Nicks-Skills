[CmdletBinding()]
param(
    [string]$TatviewerPath = (Join-Path $PSScriptRoot '..\..\tatviewer.exe'),
    [string]$TatPath = (Join-Path $PSScriptRoot '..\..\tat.exe'),
    [string]$OutputPath = (Join-Path $PSScriptRoot 'tatviewer-output.html')
)

$fixtureTasksPath = Join-Path $PSScriptRoot 'tasks'
$resolvedTatviewerPath = [System.IO.Path]::GetFullPath($TatviewerPath)
$resolvedTatPath = [System.IO.Path]::GetFullPath($TatPath)
$resolvedOutputPath = [System.IO.Path]::GetFullPath($OutputPath)
$temporaryRoot = [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath())
$scratchPath = Join-Path $temporaryRoot ("tatviewer-fixture-" + [System.Guid]::NewGuid().ToString('N'))

foreach ($requiredPath in @($fixtureTasksPath, $resolvedTatviewerPath, $resolvedTatPath)) {
    if (-not (Test-Path -LiteralPath $requiredPath)) {
        throw "Required fixture path does not exist: $requiredPath"
    }
}

if (-not $scratchPath.StartsWith($temporaryRoot, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to use a scratch directory outside the system temporary directory: $scratchPath"
}

New-Item -ItemType Directory -Path $scratchPath | Out-Null
try {
    New-Item -ItemType Directory -Path (Join-Path $scratchPath '.git') | Out-Null
    Copy-Item -LiteralPath $fixtureTasksPath -Destination $scratchPath -Recurse

    Push-Location -LiteralPath $scratchPath
    try {
        $validationOutput = & $resolvedTatPath --color never check 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Fixture validation failed:`n$($validationOutput -join [Environment]::NewLine)"
        }

        $viewerOutput = & $resolvedTatviewerPath --tat-path $resolvedTatPath --all `
            --output-path $resolvedOutputPath --force 2>&1
        if ($LASTEXITCODE -ne 0) {
            throw "Fixture rendering failed:`n$($viewerOutput -join [Environment]::NewLine)"
        }
    }
    finally {
        Pop-Location
    }

    if (-not (Test-Path -LiteralPath $resolvedOutputPath -PathType Leaf)) {
        throw "tatviewer did not create the expected output: $resolvedOutputPath"
    }

    $html = Get-Content -LiteralPath $resolvedOutputPath -Raw
    if (-not $html.Contains('<script id="task-data" type="application/json">')) {
        throw "Generated output does not contain embedded task data: $resolvedOutputPath"
    }

    $taskCount = (Get-ChildItem -LiteralPath $fixtureTasksPath -File -Filter '*.md').Count
    Write-Output "Generated $resolvedOutputPath from $taskCount fixture tasks."
}
finally {
    if (Test-Path -LiteralPath $scratchPath) {
        Remove-Item -LiteralPath $scratchPath -Recurse -Force
    }
}
