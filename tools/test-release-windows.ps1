param(
    [Parameter(Mandatory)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [Parameter(Mandatory)][ValidatePattern('^v\d+\.\d+\.\d+$')][string]$Baseline,
    [Parameter(Mandatory)][string]$BaselineDir
)

$ErrorActionPreference = 'Stop'
$dist = (Resolve-Path (Join-Path $PSScriptRoot '../dist')).Path
$baselineRoot = (Resolve-Path -LiteralPath $BaselineDir).Path
$previous = $Baseline.Substring(1)
$setupName = "HyperCast-$Version-windows-x64-setup.exe"
$zipName = "HyperCast-$Version-windows-x64-portable.zip"
$baselineName = "HyperCast-$previous-windows-x64-setup.exe"
$profileDir = Join-Path $env:LOCALAPPDATA 'HyperCast'
$appDir = Join-Path $env:LOCALAPPDATA 'Programs/HyperCast'
$installed = Join-Path $appDir 'hypercast.exe'
$registration = 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/{34FA3F08-63D0-4A64-9E61-F79E7078B39D}_is1'
if ((Test-Path -LiteralPath $profileDir) -or (Test-Path -LiteralPath $appDir) -or (Test-Path -LiteralPath $registration)) {
    throw 'Use a clean Windows runner or Sandbox for the release package tests'
}

function Confirm-Checksum([string]$Directory, [string]$FileName) {
    $file = Join-Path $Directory $FileName
    $hash = (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash.ToLowerInvariant()
    if ("$hash  $FileName" -notin (Get-Content -LiteralPath (Join-Path $Directory 'SHA256SUMS.txt'))) {
        throw "Checksum mismatch for $FileName"
    }
}

function Invoke-Setup([string]$Installer, [string]$ExpectedVersion, [string]$LogName) {
    $log = Join-Path $testDir $LogName
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/LANG=english', ('/LOG="' + $log + '"'))
    $process = Start-Process -FilePath $Installer -ArgumentList $arguments -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0) {
        Get-Content -LiteralPath $log -Tail 20
        throw "Installer exited with code $($process.ExitCode)"
    }
    if ((Get-ItemProperty -LiteralPath $registration).DisplayVersion -ne $ExpectedVersion) {
        throw "Installer did not register $ExpectedVersion"
    }
}

function Invoke-Uninstall {
    $arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART')
    $process = Start-Process -FilePath (Join-Path $appDir 'unins000.exe') -ArgumentList $arguments -Wait -PassThru -WindowStyle Hidden
    if ($process.ExitCode -ne 0 -or (Test-Path -LiteralPath $registration) -or (Test-Path -LiteralPath $installed)) {
        throw 'Uninstall left the application or its registration'
    }
}

function Confirm-ClientLaunch([string]$Executable) {
    $process = Start-Process -FilePath $Executable -ArgumentList '--help' -PassThru -WindowStyle Hidden
    if (-not $process.WaitForExit(15000) -or $process.ExitCode -ne 0) {
        throw "Packaged executable did not start: $Executable"
    }
}

Confirm-Checksum $dist $setupName
Confirm-Checksum $dist $zipName
Confirm-Checksum $baselineRoot $baselineName
$testDir = (New-Item -ItemType Directory -Path (Join-Path ([IO.Path]::GetTempPath()) ("hypercast-release-" + [guid]::NewGuid()))).FullName
$portableDir = Join-Path $testDir 'portable'
Expand-Archive -LiteralPath (Join-Path $dist $zipName) -DestinationPath $portableDir
$portable = Join-Path $portableDir "HyperCast-$Version-windows-x64/hypercast.exe"
$candidateHash = (Get-FileHash -LiteralPath $portable).Hash
Confirm-ClientLaunch $portable

$candidateSetup = Join-Path $dist $setupName
Invoke-Setup $candidateSetup $Version 'clean-install.log'
if ((Get-FileHash -LiteralPath $installed).Hash -ne $candidateHash) { throw 'Installed and portable executables differ' }
Confirm-ClientLaunch $installed
Invoke-Uninstall
Write-Output 'PASS: clean Windows install, portable launch, and uninstall'

Invoke-Setup (Join-Path $baselineRoot $baselineName) $previous 'baseline-install.log'
Confirm-ClientLaunch $installed
$fixtures = @{
    'display-name' = 'ReleaseQA'
    'language' = 'pt-BR'
    'preferences.json' = '{"schema":1,"resolution":"1080p","fps":60,"upload_mbps":12,"rate_mode":"manual","volume":37,"muted":false,"sidebar":false,"self_preview":true,"grid":true}'
}
foreach ($name in $fixtures.Keys) {
    [IO.File]::WriteAllText((Join-Path $profileDir $name), $fixtures[$name])
}
Invoke-Setup $candidateSetup $Version 'upgrade.log'
if ((Get-FileHash -LiteralPath $installed).Hash -ne $candidateHash) { throw 'Upgrade did not install the candidate executable' }
foreach ($name in $fixtures.Keys) {
    if ([IO.File]::ReadAllText((Join-Path $profileDir $name)) -cne $fixtures[$name]) {
        throw "Upgrade changed $name"
    }
}
Confirm-ClientLaunch $installed
Invoke-Uninstall
Write-Output "PASS: Windows upgrade from $Baseline with saved profile"
