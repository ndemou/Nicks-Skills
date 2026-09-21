[CmdletBinding()]
param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidateNotNullOrEmpty()]
    [string]$ScriptPath,

    [Parameter(Mandatory = $true, Position = 1)]
    [Alias('DestinationPath', 'DropPath')]
    [ValidateNotNullOrEmpty()]
    [string]$RunnerDirectory,

    [Parameter(Position = 2)]
    [ValidateRange(1, 2147483647)]
    [int]$TimeoutSeconds = 120,

    [Parameter(Position = 3)]
    [Alias('OutputLimitKiB')]
    [ValidateRange(1, 2147483647)]
    [int]$MaximumOutputKiB = 50
)

# Compatible with Windows PowerShell 5.1 and PowerShell 7.
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

function Get-UniqueTimestampedPath {
    param(
        [string]$Directory,
        [string]$Prefix,
        [string]$Suffix
    )

    do {
        $timestamp = (Get-Date).ToString('yyyy-MM-dd.HH.mm.ss')
        $path = Join-Path $Directory ($Prefix + $timestamp + $Suffix)

        if (Test-Path -LiteralPath $path) {
            Start-Sleep -Milliseconds 1000
        }
    }
    while (Test-Path -LiteralPath $path)

    return $path
}

$sourceFile = Get-Item -LiteralPath $ScriptPath -ErrorAction Stop
if ($sourceFile.PSIsContainer) {
    throw "ScriptPath must name a file: $ScriptPath"
}

$runnerDirectoryItem = Get-Item -LiteralPath $RunnerDirectory -ErrorAction Stop
if (-not $runnerDirectoryItem.PSIsContainer) {
    throw "RunnerDirectory must name a directory: $RunnerDirectory"
}

$runnerDirectoryPath = $runnerDirectoryItem.FullName
$extension = $sourceFile.Extension.ToLowerInvariant()

switch ($extension) {
    '.ps1' { $submittedName = 'code-to-run.ps1' }
    '.cmd' { $submittedName = 'code-to-run.cmd' }
    '.bat' { $submittedName = 'code-to-run.bat' }
    default {
        throw "Unsupported script extension '$extension'. Use .ps1, .cmd, or .bat."
    }
}

$submittedPath = Join-Path $runnerDirectoryPath $submittedName
$outputPath = Join-Path $runnerDirectoryPath 'code-output.txt'
$configPath = Join-Path $runnerDirectoryPath 'config.json'
$lockPath = Join-Path $runnerDirectoryPath '.master-controller.lock'
$pendingPaths = @(
    (Join-Path $runnerDirectoryPath 'code-to-run.ps1'),
    (Join-Path $runnerDirectoryPath 'code-to-run.cmd'),
    (Join-Path $runnerDirectoryPath 'code-to-run.bat')
)

$lockStream = $null
$ownsLock = $false
$stagingPath = $null

try {
    try {
        $lockStream = [System.IO.File]::Open(
            $lockPath,
            [System.IO.FileMode]::OpenOrCreate,
            [System.IO.FileAccess]::ReadWrite,
            [System.IO.FileShare]::None
        )
        $ownsLock = $true
    }
    catch {
        throw "Another Master-Controller is already using '$runnerDirectoryPath'."
    }

    foreach ($pendingPath in $pendingPaths) {
        if (Test-Path -LiteralPath $pendingPath) {
            throw "The runner already has a pending script: $pendingPath"
        }
    }

    # Publish the complete configuration before the script. The worker reads
    # this file once per command, so its limit can change without a restart.
    $config = @{
        maximumOutputKiB = $MaximumOutputKiB
    }
    $configJson = ConvertTo-Json -InputObject $config -Compress
    $stagingName = "config-uploading-by-master-" + $PID + "-" + [Guid]::NewGuid().ToString('N') + ".tmp"
    $stagingPath = Join-Path $runnerDirectoryPath $stagingName
    [System.IO.File]::WriteAllText($stagingPath, $configJson)

    Move-Item -LiteralPath $stagingPath -Destination $configPath -Force
    $stagingPath = $null

    # Move any unconsumed result out of the well-known output path before the
    # new request is submitted, so only the new result can satisfy this call.
    if (Test-Path -LiteralPath $outputPath) {
        if (-not (Test-Path -LiteralPath $outputPath -PathType Leaf)) {
            throw "The output path is not a file: $outputPath"
        }

        $previousOutputPath = Get-UniqueTimestampedPath `
            -Directory $runnerDirectoryPath `
            -Prefix 'code-output-at-' `
            -Suffix '.txt'
        [System.IO.File]::Move($outputPath, $previousOutputPath)
    }

    # Copy to a name ignored by the runner, then publish the complete script
    # with one same-directory rename.
    $stagingName = "code-to-run-uploading-by-master-" + $PID + "-" + [Guid]::NewGuid().ToString('N') + ".tmp"
    $stagingPath = Join-Path $runnerDirectoryPath $stagingName
    [System.IO.File]::Copy($sourceFile.FullName, $stagingPath, $false)
    [System.IO.File]::Move($stagingPath, $submittedPath)
    $stagingPath = $null

    $timer = [System.Diagnostics.Stopwatch]::StartNew()
    while (-not [System.IO.File]::Exists($outputPath)) {
        if ($timer.Elapsed.TotalSeconds -ge $TimeoutSeconds) {
            throw "Timed out after $TimeoutSeconds seconds waiting for '$outputPath'."
        }

        Start-Sleep -Milliseconds 100
    }

    # The worker publishes this name only after the output file is complete.
    $outputText = [System.IO.File]::ReadAllText($outputPath)
    [Console]::Out.Write($outputText)
    [Console]::Out.Flush()

    $consumedOutputPath = Get-UniqueTimestampedPath `
        -Directory $runnerDirectoryPath `
        -Prefix 'code-output-consumed-at-' `
        -Suffix '.txt'
    [System.IO.File]::Move($outputPath, $consumedOutputPath)
}
finally {
    if (($stagingPath -ne $null) -and ([System.IO.File]::Exists($stagingPath))) {
        try {
            [System.IO.File]::Delete($stagingPath)
        }
        catch {
        }
    }

    if ($lockStream -ne $null) {
        $lockStream.Dispose()
    }

    if ($ownsLock) {
        try {
            [System.IO.File]::Delete($lockPath)
        }
        catch {
        }
    }
}
