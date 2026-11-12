$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcDestination = "$hcRoot/.tools/ffmpeg"
if (Test-Path -LiteralPath "$hcDestination/include/libavcodec/avcodec.h") { return }
$hcArchive = "$hcRoot/.tools/ffmpeg-sdk.zip"
$hcSdk = 'ffmpeg-n9.0.2-39-g7adfbe40dc-win64-lgpl-shared-9.0'
New-Item -ItemType Directory -Force "$hcRoot/.tools" | Out-Null
# Month-end builds remain available after the daily releases expire.
Invoke-WebRequest "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/autobuild-2026-10-31-12-59/$hcSdk.zip" -OutFile $hcArchive
if ((Get-FileHash -LiteralPath $hcArchive -Algorithm SHA256).Hash.ToLower() -ne 'e907b01fa4783913151237e37df2a10fa97d99a1598a021bf59d4ae62ffed4c0') { throw 'FFmpeg SDK checksum mismatch' }
$hcExtract = "$hcRoot/.tools/ffmpeg-sdk-unpacked"
Expand-Archive -LiteralPath $hcArchive -DestinationPath $hcExtract -Force
New-Item -ItemType Directory -Force $hcDestination | Out-Null
$hcSource = Join-Path $hcExtract $hcSdk
Copy-Item -Recurse -LiteralPath "$hcSource/include","$hcSource/lib","$hcSource/bin","$hcSource/doc" -Destination $hcDestination
Copy-Item -LiteralPath "$hcSource/LICENSE.txt" -Destination "$hcDestination/LICENSE.txt"
Write-Output 'Pinned LGPL FFmpeg SDK ready; GPU decode is D3D11VA only.'
