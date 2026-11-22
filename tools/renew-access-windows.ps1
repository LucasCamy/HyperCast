$ErrorActionPreference = 'Stop'

# Private operator values stay out of Git: .tools/ is ignored.
$settings = Join-Path (Split-Path -Parent $PSScriptRoot) '.tools/renew-access.env'
$values = @{}
if (Test-Path -LiteralPath $settings) {
    foreach ($line in Get-Content -LiteralPath $settings) {
        if ($line -match '^\s*([A-Z_]+)\s*=\s*(.+?)\s*$') { $values[$Matches[1]] = $Matches[2] }
    }
}
$server = $values['HYPERCAST_RENEW_SSH']
$accessId = $values['HYPERCAST_RENEW_ACCESS_ID']
$renewalScript = Join-Path $PSScriptRoot 'renew-access.py'
$command = "sudo -n k3s kubectl -n hypercast exec -i deploy/hypercast-access -c auth -- python3 - $accessId --days 30"

try {
    if (-not $server -or $accessId -notmatch '^[0-9a-f]{32}$') {
        throw "Crie $settings com HYPERCAST_RENEW_SSH=usuario@ip-da-vm e HYPERCAST_RENEW_ACCESS_ID=<access_id de 32 caracteres>."
    }
    if (-not (Get-Command ssh.exe -ErrorAction SilentlyContinue)) {
        throw 'O cliente OpenSSH do Windows precisa estar instalado.'
    }
    $script = Get-Content -LiteralPath $renewalScript -Raw

    Write-Host 'Renovando a chave atual por 30 dias.'
    Write-Host 'Se o SSH pedir senha, digite a senha da VM e pressione Enter.'
    $json = $script | & ssh.exe -T -o StrictHostKeyChecking=yes -o ConnectTimeout=10 $server $command
    if ($LASTEXITCODE -ne 0) {
        throw "O servidor encerrou o comando com codigo $LASTEXITCODE. Confira o acesso SSH e a mensagem acima."
    }

    $result = $json | ConvertFrom-Json
    if (-not $result.expires_at -or $result.changed -isnot [bool]) {
        throw 'O servidor retornou uma validade inesperada.'
    }
    $utc = [DateTimeOffset]::Parse($result.expires_at, [Globalization.CultureInfo]::InvariantCulture)
    $cuiaba = [TimeZoneInfo]::FindSystemTimeZoneById('Central Brazilian Standard Time')
    $expires = [TimeZoneInfo]::ConvertTime($utc, $cuiaba).ToString('dd/MM/yyyy HH:mm:ss')
    $status = if ($result.changed) { 'Chave renovada.' } else { 'A chave ja tem pelo menos 30 dias de validade.' }
    Write-Host "$status Valida ate $expires (horario de Cuiaba)."
    Write-Host 'Se o HyperCast mostrar um erro de chave, clique em Continuar com a chave.'
    exit 0
} catch {
    Write-Host $_.Exception.Message
    exit 1
}
