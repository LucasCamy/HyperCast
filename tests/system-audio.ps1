$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcVsWhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
$hcVsPath = & $hcVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
& "$hcVsPath/Common7/Tools/Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
Push-Location "$hcRoot/.tools"
try {
    & cl.exe /nologo /EHsc /std:c++17 /MT "$hcRoot/tests/system_audio_exclusion.cpp" "$hcRoot/native/system_audio.cpp" /Fe:system-audio-test.exe /link ole32.lib mmdevapi.lib
    if ($LASTEXITCODE -ne 0) { throw 'System audio test compilation failed' }
    & './system-audio-test.exe'
    if ($LASTEXITCODE -ne 0) { throw 'System audio test failed' }
} finally { Pop-Location }
