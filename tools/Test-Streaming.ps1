param(
    [ValidateSet('Viewer','Publisher')][string]$Role = 'Viewer',
    [ValidateSet('av1','h265')][string]$Codec = 'av1',
    [ValidateRange(10,120)][int]$Seconds = 30,
    [ValidateRange(0,15)][int]$Adapter = 0,
    [ValidateRange(0,15)][int]$Monitor = 0,
    [switch]$UseEnvironmentToken
)
$ErrorActionPreference = 'Stop'
$hcFolder = $null
$hcPointer = [IntPtr]::Zero
$hcSecureToken = $null
$hcProcess = $null
$hcStarted = $false
$hcPreviousToken = $env:HYPERCAST_ROOM_TOKEN
$hcPreviousTrace = $env:HYPERCAST_TRACE_CODEC
try {
    $hcFolder = Join-Path $env:LOCALAPPDATA ('HyperCast/diagnostics/' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $hcFolder -Force | Out-Null
    $hcExe = Join-Path $PSScriptRoot 'hypercast.exe'
    if (-not (Test-Path -LiteralPath $hcExe)) {
        throw 'Extraia TODO o ZIP antes de testar. Test-Streaming.ps1 e hypercast.exe precisam estar na mesma pasta. Nao execute dentro do ZIP.'
    }
    foreach ($hcDll in @('avcodec-63.dll','avutil-61.dll','swresample-7.dll')) {
        if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $hcDll))) {
            throw "Arquivo ausente: $hcDll. Extraia novamente o pacote completo."
        }
    }
    try {
        Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate |
            ConvertTo-Json | Set-Content -LiteralPath "$hcFolder/hardware.json" -Encoding utf8
    } catch {
        'Hardware query unavailable; streaming test can continue.' | Set-Content -LiteralPath "$hcFolder/hardware-query.log" -Encoding utf8
        Write-Warning 'Nao foi possivel consultar a GPU via WMI. O teste de video vai continuar.'
    }
    if ($UseEnvironmentToken) {
        if ([string]::IsNullOrWhiteSpace($env:HYPERCAST_ROOM_TOKEN)) { throw 'HYPERCAST_ROOM_TOKEN is missing.' }
    } else {
        $hcSecureToken = Read-Host 'Token da sala (conforme o papel escolhido)' -AsSecureString
        $hcPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($hcSecureToken)
        $env:HYPERCAST_ROOM_TOKEN = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($hcPointer)
        if ([string]::IsNullOrWhiteSpace($env:HYPERCAST_ROOM_TOKEN)) { throw 'Token vazio. Gere um token valido para esta sala.' }
    }
    $env:HYPERCAST_TRACE_CODEC = $null
    $hcArguments = @('--seconds', "$Seconds")
    if ($Role -eq 'Publisher') {
        Write-Output 'Compartilhando o monitor selecionado durante o teste. Nenhum video sera gravado.'
        $hcArguments += @('--encode','--codec',$Codec,'--fps','120','--adapter',"$Adapter",'--output',"$Monitor",'--publish-url','wss://hypercast.cloudlane.com.br')
    } else {
        $hcArguments += @('--receive-url','wss://hypercast.cloudlane.com.br','--view-mode','spotlight')
    }
    # Arguments are fixed strings or validated integers/enums. No shell.
    $hcStart = New-Object System.Diagnostics.ProcessStartInfo
    $hcStart.FileName = $hcExe
    $hcStart.WorkingDirectory = $PSScriptRoot
    $hcStart.Arguments = $hcArguments -join ' '
    $hcStart.UseShellExecute = $false
    $hcStart.CreateNoWindow = $true
    $hcStart.RedirectStandardOutput = $true
    $hcStart.RedirectStandardError = $true
    $hcProcess = New-Object System.Diagnostics.Process
    $hcProcess.StartInfo = $hcStart
    if (-not $hcProcess.Start()) { throw 'Nao foi possivel iniciar hypercast.exe.' }
    $hcStarted = $true
    # Drain both pipes concurrently; native stderr cannot abort PowerShell 5.1.
    $hcStdout = $hcProcess.StandardOutput.ReadToEndAsync()
    $hcStderr = $hcProcess.StandardError.ReadToEndAsync()
    if (-not $hcProcess.WaitForExit(($Seconds + 60) * 1000)) {
        $hcProcess.Kill()
        $hcProcess.WaitForExit()
        throw 'O teste excedeu o tempo limite.'
    }
    $hcLog = ($hcStdout.GetAwaiter().GetResult() + $hcStderr.GetAwaiter().GetResult()).Replace($env:HYPERCAST_ROOM_TOKEN,'[token]')
    $hcLog | Set-Content -LiteralPath "$hcFolder/stream.log" -Encoding utf8
    Write-Output $hcLog
    if ($hcProcess.ExitCode -ne 0) { throw "HyperCast falhou (codigo $($hcProcess.ExitCode)). Consulte stream.log." }
} catch {
    $hcMessage = $_.Exception.Message
    if (-not [string]::IsNullOrEmpty($env:HYPERCAST_ROOM_TOKEN)) {
        $hcMessage = $hcMessage.Replace($env:HYPERCAST_ROOM_TOKEN,'[token]')
    }
    if ($hcFolder -and (Test-Path -LiteralPath $hcFolder)) {
        $hcMessage | Set-Content -LiteralPath "$hcFolder/error.log" -Encoding utf8
    }
    Write-Host ("ERRO: " + $hcMessage) -ForegroundColor Red
    throw $hcMessage
} finally {
    $env:HYPERCAST_ROOM_TOKEN = $hcPreviousToken
    $env:HYPERCAST_TRACE_CODEC = $hcPreviousTrace
    if ($hcPointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($hcPointer) }
    if ($hcSecureToken) { $hcSecureToken.Dispose() }
    if ($hcProcess) {
        if ($hcStarted -and -not $hcProcess.HasExited) {
            $hcProcess.Kill()
            $hcProcess.WaitForExit()
        }
        $hcProcess.Dispose()
    }
    if ($hcFolder) { Write-Host "Logs: $hcFolder" }
}
