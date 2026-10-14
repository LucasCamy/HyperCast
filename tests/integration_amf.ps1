$ErrorActionPreference='Stop'
$hcRoot=Split-Path -Parent $PSScriptRoot
$hcVsWhere='C:/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
$hcVsPath=& $hcVsWhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
& "$hcVsPath/Common7/Tools/Launch-VsDevShell.ps1" -Arch amd64 -HostArch amd64 -SkipAutomaticLocation | Out-Null
$hcId=[guid]::NewGuid().ToString('N')
$hcBinary="$hcRoot/.tools/amf-probe-$hcId.exe"
$hcOutput="$hcRoot/.tools/amf-probe-$hcId.h265"
Push-Location "$hcRoot/.tools"
try {
    & cl.exe /nologo /std:c++20 /EHsc /utf-8 /MT "/I$hcRoot/native/vendor" "$hcRoot/tests/amf_probe.cpp" "$hcRoot/native/amf.cpp" "/Fe:$hcBinary" /link d3d11.lib dxgi.lib d3dcompiler.lib ole32.lib
    if ($LASTEXITCODE -ne 0) { throw 'AMF probe compilation failed' }
    & $hcBinary $hcOutput
    if ($LASTEXITCODE -ne 0) { throw 'AMD hardware encoder probe failed' }
    & "$hcRoot/.tools/ffmpeg/bin/ffmpeg.exe" -hide_banner -loglevel error -hwaccel d3d11va -hwaccel_output_format d3d11 -i $hcOutput -f null -
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic HEVC GPU decode failed' }
    $hcFrames=& "$hcRoot/.tools/ffmpeg/bin/ffprobe.exe" -v error -count_frames -select_streams v:0 -show_entries stream=nb_read_frames -of csv=p=0 $hcOutput
    if ($LASTEXITCODE -ne 0 -or "$hcFrames" -ne '120') { throw 'AMF synthetic stream frame count mismatch' }
    Write-Output 'PASS: synthetic AMF stream decodes through D3D11VA, 120 frames verified'
} finally {
    Pop-Location
    # Only files created by this invocation, all under the explicit workspace.
    foreach ($hcPath in @($hcOutput,$hcBinary)) {if(Test-Path -LiteralPath $hcPath){Remove-Item -LiteralPath $hcPath}}
}
