$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcTest = [IO.Path]::GetFullPath((Join-Path $hcRoot '.tools/install-test'))
if (-not $hcTest.StartsWith([IO.Path]::GetFullPath($hcRoot) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Install-test target escaped workspace' }
$hcRegistry = 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall/{34FA3F08-63D0-4A64-9E61-F79E7078B39D}_is1'
if (Test-Path $hcRegistry) { throw 'Existing HyperCast installation found; do not overwrite it for a test' }
if (Get-Process hypercast -ErrorAction SilentlyContinue) { throw 'Close HyperCast before testing its installer' }
$hcInstaller = "$hcRoot/dist/HyperCast-0.8.0-windows-x64-setup.exe"
$hcProcess = Start-Process -FilePath $hcInstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART',"/DIR=`"$hcTest`"","/LOG=`"$hcRoot/.tools/install.log`"") -WindowStyle Hidden -Wait -PassThru
if ($hcProcess.ExitCode -ne 0) { throw "Installer failed ($($hcProcess.ExitCode))" }
$hcPreviousExe = $env:HYPERCAST_TEST_EXE
try {
    foreach ($hcName in @('hypercast.exe','avcodec-63.dll','avutil-61.dll','swresample-7.dll','unins000.exe','licenses/THIRD_PARTY.md')) {
        if (-not (Test-Path -LiteralPath "$hcTest/$hcName")) { throw "Installed file missing: $hcName" }
    }
    $env:HYPERCAST_TEST_EXE = "$hcTest/hypercast.exe"
    python -m tests.integration_media
    if ($LASTEXITCODE -ne 0) { throw 'Installed native GPU media test failed' }
} finally {
    $env:HYPERCAST_TEST_EXE = $hcPreviousExe
    # Uninstaller is allowed only for the explicit, verified test directory.
    $hcUninstaller = [IO.Path]::GetFullPath((Join-Path $hcTest 'unins000.exe'))
    if (-not $hcUninstaller.StartsWith($hcTest + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) { throw 'Unsafe uninstaller target' }
    $hcProcess = Start-Process -FilePath $hcUninstaller -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -WindowStyle Hidden -Wait -PassThru
    if ($hcProcess.ExitCode -ne 0) { throw 'Test uninstaller failed' }
}
if ((Test-Path $hcRegistry) -or (Test-Path -LiteralPath "$hcTest/hypercast.exe")) { throw 'Uninstall left application registration/files' }
Write-Output 'PASS: per-user install, packaged AV1 GPU decode and clean uninstall'
