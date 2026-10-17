param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
if (-not $SkipBuild) { & "$PSScriptRoot/cargo-native.ps1" build --release --locked --features desktop }
& "$PSScriptRoot/cargo-native.ps1" metadata --locked --features desktop --filter-platform x86_64-pc-windows-msvc --format-version 1 | Set-Content -Encoding utf8 "$hcRoot/.tools/package-metadata.json"
python "$PSScriptRoot/collect_licenses.py"
if ($LASTEXITCODE -ne 0) { throw 'Dependency notice collection failed' }
$hcDist = "$hcRoot/dist"
$hcStage = "$hcDist/HyperCast-0.8.13-windows-x64"
# No recursive cleanup: stage a fixed allowlist and refuse stale unexpected files.
New-Item -ItemType Directory -Force "$hcStage/licenses" | Out-Null
Copy-Item -LiteralPath "$hcRoot/target/x86_64-pc-windows-msvc/release/hypercast.exe" -Destination $hcStage
Copy-Item -LiteralPath "$PSScriptRoot/Test-Streaming.ps1" -Destination $hcStage
Copy-Item -LiteralPath "$PSScriptRoot/Test-Publisher.cmd","$PSScriptRoot/Test-Viewer.cmd" -Destination $hcStage
foreach ($hcDll in @('avcodec-63.dll','avutil-61.dll','swresample-7.dll')) {
    Copy-Item -LiteralPath "$hcRoot/.tools/ffmpeg/bin/$hcDll" -Destination $hcStage
}
Copy-Item -LiteralPath "$hcRoot/LICENSE","$hcRoot/docs/THIRD_PARTY.md","$hcRoot/docs/portable.md","$hcRoot/docs/updates.md","$hcRoot/docs/room-creation.md","$hcRoot/docs/amd.md","$hcRoot/docs/video-quality.md" -Destination "$hcStage/licenses"
$hcLgpl = "$hcRoot/.tools/ffmpeg/LICENSE.txt"
if (-not (Test-Path -LiteralPath $hcLgpl)) {
    Copy-Item -Path "$hcRoot/.tools/ffmpeg-sdk-unpacked/*/LICENSE.txt" -Destination $hcLgpl
}
Copy-Item -LiteralPath $hcLgpl -Destination "$hcStage/licenses/FFmpeg-LGPL-3.0.txt"
Copy-Item -LiteralPath "$hcRoot/vendor/webrtc-sys/NOTICE.md" -Destination "$hcStage/licenses/LiveKit-NOTICE.md"
$hcWebRtcLicense = & rg --files "$hcRoot/target/release/build" -g LICENSE.md | Where-Object { $_ -match 'livekit_webrtc' } | Select-Object -First 1
if (-not $hcWebRtcLicense) { throw 'Prebuilt WebRTC third-party license is missing' }
Copy-Item -LiteralPath $hcWebRtcLicense -Destination "$hcStage/licenses/WebRTC-LICENSE.md"
Copy-Item -LiteralPath "$hcRoot/native/vendor/amf/LICENSE.txt" -Destination "$hcStage/licenses/AMF-LICENSE.txt"
Copy-Item -LiteralPath "$hcRoot/native/vendor/nvEncodeAPI.h" -Destination "$hcStage/licenses/nvEncodeAPI.h"
& "$hcRoot/.tools/ffmpeg/bin/ffmpeg.exe" -version | Set-Content -Encoding utf8 "$hcStage/licenses/FFmpeg-build.txt"
$hcSource = "$hcRoot/.tools/corresponding-source"
New-Item -ItemType Directory -Force $hcSource | Out-Null
foreach ($hcLicense in @('COPYING.GPLv3','COPYING.LGPLv3')) {
    $hcFile = "$hcSource/$hcLicense"
    if (-not (Test-Path -LiteralPath $hcFile)) { Invoke-WebRequest "https://raw.githubusercontent.com/FFmpeg/FFmpeg/7e93014c5b/$hcLicense" -OutFile $hcFile }
    Copy-Item -LiteralPath $hcFile -Destination "$hcStage/licenses/$hcLicense.txt"
}
$hcZip = "$hcDist/HyperCast-0.8.13-windows-x64-portable.zip"
$hcAllowed = @('hypercast.exe','Test-Streaming.ps1','Test-Publisher.cmd','Test-Viewer.cmd','avcodec-63.dll','avutil-61.dll','swresample-7.dll',
    'licenses/LICENSE','licenses/THIRD_PARTY.md','licenses/portable.md','licenses/updates.md','licenses/room-creation.md','licenses/amd.md','licenses/video-quality.md',
    'licenses/FFmpeg-LGPL-3.0.txt','licenses/LiveKit-NOTICE.md','licenses/WebRTC-LICENSE.md',
    'licenses/AMF-LICENSE.txt','licenses/nvEncodeAPI.h','licenses/FFmpeg-build.txt','licenses/COPYING.GPLv3.txt',
    'licenses/COPYING.LGPLv3.txt','licenses/Rust-dependencies.txt')
foreach ($hcFile in Get-ChildItem -LiteralPath $hcStage -File -Recurse) {
    $hcRelative = [IO.Path]::GetRelativePath($hcStage, $hcFile.FullName).Replace('\','/')
    if ($hcRelative -notin $hcAllowed) { throw "Unexpected staging file: $hcRelative" }
}
Compress-Archive -LiteralPath $hcStage -DestinationPath $hcZip -Force
$hcIsccCandidates = @("$env:LOCALAPPDATA/Programs/Inno Setup 6/ISCC.exe",'C:/Program Files (x86)/Inno Setup 6/ISCC.exe')
$hcIscc = $hcIsccCandidates | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if (-not $hcIscc) { throw 'Install Inno Setup: winget install --id JRSoftware.InnoSetup --exact' }
& $hcIscc "/DStageDir=$hcStage" "/DOutDir=$hcDist" "$hcRoot/installer/HyperCast.iss"
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup compilation failed' }
Get-FileHash -Algorithm SHA256 -LiteralPath $hcZip,"$hcDist/HyperCast-0.8.13-windows-x64-setup.exe" | ForEach-Object { "$($_.Hash.ToLower())  $(Split-Path -Leaf $_.Path)" } | Set-Content -Encoding ascii "$hcDist/SHA256SUMS.txt"
Write-Output "Portable archive and per-user installer generated in $hcDist"
