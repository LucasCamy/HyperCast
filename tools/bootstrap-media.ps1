$ErrorActionPreference = 'Stop'
$hcRoot = Split-Path -Parent $PSScriptRoot
$hcDestination = "$hcRoot/.tools/ffmpeg"
if (Test-Path -LiteralPath "$hcDestination/include/libavcodec/avcodec.h") { return }
$hcArchive = "$hcRoot/.tools/ffmpeg-sdk.zip"
New-Item -ItemType Directory -Force "$hcRoot/.tools" | Out-Null
Invoke-WebRequest 'https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-10-12-13-12/ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0.zip' -OutFile $hcArchive
if ((Get-FileHash -LiteralPath $hcArchive -Algorithm SHA256).Hash.ToLower() -ne 'de3f3b16b3dbf2038a56878aa35795f48bd220f01479697e008470773cecba5b') { throw 'FFmpeg SDK checksum mismatch' }
$hcExtract = "$hcRoot/.tools/ffmpeg-sdk-unpacked"
Expand-Archive -LiteralPath $hcArchive -DestinationPath $hcExtract -Force
New-Item -ItemType Directory -Force $hcDestination | Out-Null
Copy-Item -Recurse -LiteralPath "$hcExtract/ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0/include","$hcExtract/ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0/lib","$hcExtract/ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0/bin","$hcExtract/ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0/doc" -Destination $hcDestination
Copy-Item -LiteralPath "$hcExtract/ffmpeg-n9.0.2-27-g7e93014c5b-win64-lgpl-shared-9.0/LICENSE.txt" -Destination "$hcDestination/LICENSE.txt"
Write-Output 'Pinned LGPL FFmpeg SDK ready; GPU decode is D3D11VA only.'
