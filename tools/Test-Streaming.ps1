param(
    [ValidateSet('Viewer','Publisher')][string]$Role = 'Viewer',
    [ValidateSet('av1','h265')][string]$Codec = 'av1',
    [ValidateRange(10,120)][int]$Seconds = 30,
    [ValidateRange(0,15)][int]$Adapter = 0,
    [ValidateRange(0,15)][int]$Monitor = 0
)
$ErrorActionPreference = 'Stop'
$hcExe = Join-Path $PSScriptRoot 'hypercast.exe'
if (-not (Test-Path -LiteralPath $hcExe)) { throw 'Run this script from the extracted HyperCast portable package' }
$hcFolder = Join-Path $env:LOCALAPPDATA ('HyperCast/diagnostics/' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $hcFolder -Force | Out-Null
Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate |
    ConvertTo-Json | Set-Content -LiteralPath "$hcFolder/hardware.json" -Encoding utf8
$hcSecureToken = Read-Host 'Room token (viewer or publisher matching the chosen role)' -AsSecureString
$hcPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($hcSecureToken)
$hcPreviousToken = $env:HYPERCAST_ROOM_TOKEN
$hcPreviousTrace = $env:HYPERCAST_TRACE_CODEC
try {
    $env:HYPERCAST_ROOM_TOKEN = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($hcPointer)
    $env:HYPERCAST_TRACE_CODEC = $null
    $hcArguments = @('--seconds', "$Seconds")
    if ($Role -eq 'Publisher') {
        Write-Output 'Sharing the selected monitor for the test; no video file is recorded.'
        $hcArguments += @('--encode','--codec',$Codec,'--fps','120','--adapter',"$Adapter",'--output',"$Monitor",'--publish-url','wss://hypercast.cloudlane.com.br')
    } else {
        $hcArguments += @('--receive-url','wss://hypercast.cloudlane.com.br','--view-mode','spotlight')
    }
    $hcLog = & $hcExe @hcArguments 2>&1 | ForEach-Object { $_.ToString().Replace($env:HYPERCAST_ROOM_TOKEN,'[token]') }
    $hcExit = $LASTEXITCODE
    $hcLog | Set-Content -LiteralPath "$hcFolder/stream.log" -Encoding utf8
    $hcLog | Write-Output
    Write-Output "Diagnostics saved to $hcFolder (hardware and text counters, no token or video recording)."
    if ($hcExit -ne 0) { throw "HyperCast test failed ($hcExit); inspect stream.log" }
} finally {
    $env:HYPERCAST_ROOM_TOKEN = $hcPreviousToken
    $env:HYPERCAST_TRACE_CODEC = $hcPreviousTrace
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($hcPointer)
    $hcSecureToken.Dispose()
}
