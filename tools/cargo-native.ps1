param([Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArgs)
$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcVsWhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
$hcVsPath = & $hcVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $hcVsPath) { throw 'Visual Studio C++ tools are not installed' }
& "$hcVsPath/Common7/Tools/Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
& "$PSScriptRoot/bootstrap-media.ps1"
$env:HYPERCAST_FFMPEG_DIR = "$hcRoot/.tools/ffmpeg"
$env:PATH = "$env:HYPERCAST_FFMPEG_DIR/bin;$env:PATH"
$hcLocalCargo = "$hcRoot/.tools/cargo/bin/cargo.exe"
if (Test-Path -LiteralPath $hcLocalCargo) {
    $env:RUSTUP_HOME = "$hcRoot/.tools/rustup"
    $env:CARGO_HOME = "$hcRoot/.tools/cargo"
    $hcCargo = $hcLocalCargo
} else {
    $hcCargo = (Get-Command cargo.exe -ErrorAction Stop).Source
}
if (-not $CargoArgs) { $CargoArgs = @('check', '--locked') }
if ($CargoArgs[0] -in @('build', 'check', 'test', 'run', 'clippy')) {
    $CargoArgs = @($CargoArgs[0], '--target', 'x86_64-pc-windows-msvc') + @($CargoArgs | Select-Object -Skip 1)
}
Push-Location $hcRoot
try {
    & $hcCargo @CargoArgs
    if ($LASTEXITCODE -ne 0) { throw "cargo failed ($LASTEXITCODE)" }
    foreach ($hcProfile in @('debug', 'release')) {
        $hcOutput = "$hcRoot/target/x86_64-pc-windows-msvc/$hcProfile"
        if (Test-Path -LiteralPath $hcOutput) {
            Copy-Item -Path "$env:HYPERCAST_FFMPEG_DIR/bin/*.dll" -Destination $hcOutput
        }
    }
} finally { Pop-Location }
