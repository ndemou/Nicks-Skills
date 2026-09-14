# Compatible with Windows PowerShell 1.0.

function Write-RunnerMessage {
    param(
        [string]$Message,
        [string]$ForegroundColor
    )

    $timestampedMessage = '[' + (Get-Date).ToString('yyyy-MM-dd HH:mm:ss') + '] ' + $Message

    if (($ForegroundColor -eq $null) -or ($ForegroundColor.Length -eq 0)) {
        Write-Host $timestampedMessage
    }
    else {
        Write-Host $timestampedMessage -ForegroundColor $ForegroundColor
    }
}

# Watch the directory from which the runner was started, not the directory
# containing this script.
$workDirectory = (Get-Location).Path
$outputPath = Join-Path $workDirectory 'code-output.txt'
$temporaryOutputPath = Join-Path $workDirectory ("code-output-from-code-being-executed-" + $PID + ".tmp")
$pollMilliseconds = 500
$requiredUnchangedChecks = 2
$postRunDelayMilliseconds = 1000
$candidateNames = 'code-to-run.ps1, code-to-run.cmd, code-to-run.bat'
$waitingAnnounced = $false

# A .cmd or .bat input is archived with .cmd.txt, as both run through CMD.EXE.
$candidates = @(
    @{
        Path = (Join-Path $workDirectory 'code-to-run.ps1')
        Language = 'PowerShell'
        ArchiveExtension = 'ps1'
        LastSize = $null
        UnchangedChecks = 0
        File = $null
    },
    @{
        Path = (Join-Path $workDirectory 'code-to-run.cmd')
        Language = 'CMD.EXE'
        ArchiveExtension = 'cmd'
        LastSize = $null
        UnchangedChecks = 0
        File = $null
    },
    @{
        Path = (Join-Path $workDirectory 'code-to-run.bat')
        Language = 'CMD.EXE'
        ArchiveExtension = 'cmd'
        LastSize = $null
        UnchangedChecks = 0
        File = $null
    }
)

while ($true) {
    $readyCandidates = @()

    foreach ($candidate in $candidates) {
        $file = Get-Item -LiteralPath $candidate['Path'] -ErrorAction SilentlyContinue
        $candidate['File'] = $file

        if (($file -eq $null) -or $file.PSIsContainer) {
            $candidate['LastSize'] = $null
            $candidate['UnchangedChecks'] = 0
            continue
        }

        if (($candidate['LastSize'] -ne $null) -and
            ($file.Length -eq $candidate['LastSize'])) {
            $candidate['UnchangedChecks'] = $candidate['UnchangedChecks'] + 1
        }
        else {
            $candidate['LastSize'] = $file.Length
            $candidate['UnchangedChecks'] = 0
        }

        # The first reading plus two matching readings means that the size has
        # stayed unchanged for about one second.
        if ($candidate['UnchangedChecks'] -ge $requiredUnchangedChecks) {
            $readyCandidates += ,$candidate
        }
    }

    if ($readyCandidates.Count -eq 0) {
        if (-not $waitingAnnounced) {
            Write-RunnerMessage `
                -Message ("Waiting for code to appear in " + $workDirectory + " (" + $candidateNames + ").")
            $waitingAnnounced = $true
        }

        [System.Threading.Thread]::Sleep($pollMilliseconds)
        continue
    }

    $waitingAnnounced = $false

    # If more than one file is ready, run the oldest one first.
    $ready = $readyCandidates[0]
    foreach ($candidate in $readyCandidates) {
        if ($candidate['File'].LastWriteTimeUtc -lt $ready['File'].LastWriteTimeUtc) {
            $ready = $candidate
        }
    }

    $codePath = $ready['Path']
    $codeFile = Get-Item -LiteralPath $codePath -ErrorAction SilentlyContinue

    # Start the stability test again if the selected file changed or vanished.
    if (($codeFile -eq $null) -or
        ($codeFile.Length -ne $ready['LastSize'])) {
        $ready['LastSize'] = $null
        $ready['UnchangedChecks'] = 0
        continue
    }

    # Avoid overwriting a code archive if another run already used this second.
    do {
        $runTimestamp = (Get-Date).ToString('yyyy-MM-dd.HH.mm.ss')
        $archiveName = "code-run-at-" + $runTimestamp + "." + $ready['ArchiveExtension'] + ".txt"
        $archivePath = Join-Path $workDirectory $archiveName

        if (Test-Path -LiteralPath $archivePath) {
            [System.Threading.Thread]::Sleep($postRunDelayMilliseconds)
        }
    }
    while (Test-Path -LiteralPath $archivePath)

    # Preserve the previous completed output before starting the next script.
    # Choose a new timestamp after any failed move so no archive is overwritten.
    while (Test-Path -LiteralPath $outputPath) {
        do {
            $outputArchiveTimestamp = (Get-Date).ToString('yyyy-MM-dd.HH.mm.ss')
            $outputArchiveName = "code-output-at-" + $outputArchiveTimestamp + ".txt"
            $outputArchivePath = Join-Path $workDirectory $outputArchiveName

            if (Test-Path -LiteralPath $outputArchivePath) {
                [System.Threading.Thread]::Sleep($postRunDelayMilliseconds)
            }
        }
        while (Test-Path -LiteralPath $outputArchivePath)

        Move-Item -LiteralPath $outputPath -Destination $outputArchivePath -ErrorAction SilentlyContinue

        if (Test-Path -LiteralPath $outputPath) {
            Write-RunnerMessage `
                -Message 'Waiting to archive the previous completed output file.' `
                -ForegroundColor Yellow
            [System.Threading.Thread]::Sleep($pollMilliseconds)
        }
    }

    Write-RunnerMessage `
        -Message ("Running " + $ready['Language'] + " code: " + $codePath) `
        -ForegroundColor Cyan

    $previewLines = Get-Content -LiteralPath $codePath -ErrorAction SilentlyContinue |
        Where-Object { $_.Trim().Length -gt 0 } |
        Select-Object -First 5

    foreach ($line in $previewLines) {
        Write-Host $line -ForegroundColor DarkCyan
    }

    # Tee-Object does not create a file for an empty pipeline, so create an
    # empty temporary file first. Tee writes any output to this file and also
    # displays it on screen.
    [System.IO.File]::WriteAllText($temporaryOutputPath, '')

    if ($ready['Language'] -eq 'PowerShell') {
        $childCommand = "& '" + $codePath.Replace("'", "''") + "'"
        & powershell.exe -NoLogo -NoProfile -NonInteractive -Command $childCommand 2>&1 |
            tee -FilePath $temporaryOutputPath
    }
    else {
        & $env:ComSpec /D /C $codePath 2>&1 |
            tee -FilePath $temporaryOutputPath
    }

    # Publish only the completed output. Keep retrying if another program has
    # the destination open. The previous complete output remains until then.
    while (Test-Path -LiteralPath $temporaryOutputPath) {
        Move-Item -LiteralPath $temporaryOutputPath -Destination $outputPath -Force -ErrorAction SilentlyContinue

        if (Test-Path -LiteralPath $temporaryOutputPath) {
            Write-RunnerMessage `
                -Message 'Waiting to publish the completed output file.' `
                -ForegroundColor Yellow
            [System.Threading.Thread]::Sleep($pollMilliseconds)
        }
    }

    # Rename the input only after its process has finished.
    while (Test-Path -LiteralPath $codePath) {
        Move-Item -LiteralPath $codePath -Destination $archivePath -ErrorAction SilentlyContinue

        if (Test-Path -LiteralPath $codePath) {
            Write-RunnerMessage `
                -Message ("Waiting to archive " + $codePath) `
                -ForegroundColor Yellow
            [System.Threading.Thread]::Sleep($pollMilliseconds)
        }
    }

    foreach ($candidate in $candidates) {
        $candidate['LastSize'] = $null
        $candidate['UnchangedChecks'] = 0
        $candidate['File'] = $null
    }

    Write-RunnerMessage -Message 'Waiting for one second before the next script is executed.'
    [System.Threading.Thread]::Sleep($postRunDelayMilliseconds)
}
