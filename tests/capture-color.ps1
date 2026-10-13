$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcVsWhere = 'C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
$hcVsPath = & $hcVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
& "$hcVsPath/Common7/Tools/Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
Push-Location "$hcRoot/.tools"
try {
    & cl.exe /nologo /EHsc /std:c++17 /MT "/I$hcRoot/native/vendor" "$hcRoot/tests/capture_color.cpp" "$hcRoot/native/nvenc.cpp" /Fe:capture-color-test.exe /link d3d11.lib d3dcompiler.lib dxgi.lib
    if ($LASTEXITCODE -ne 0) { throw 'Capture color test compilation failed' }
    & './capture-color-test.exe'
    if ($LASTEXITCODE -ne 0) { throw 'Capture color GPU test failed' }
} finally { Pop-Location }
