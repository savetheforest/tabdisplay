# Assina um .exe/.dll com o certificado de code signing (usado pelo Tauri em bundle.windows.signCommand,
# que chama este script para o executável do app e para o instalador).
#
# O certificado vem do ambiente, nunca do repositório. Escolha um:
#   TABDISPLAY_SIGN_PFX + TABDISPLAY_SIGN_PFX_PASSWORD   arquivo .pfx (certificado OV/EV exportável)
#   TABDISPLAY_SIGN_THUMBPRINT                           certificado no repositório do Windows (token USB / EV)
# Sem nenhum dos dois, o arquivo fica sem assinatura (build local): o script avisa e sai com sucesso.
# TABDISPLAY_SIGN_TIMESTAMP muda o servidor de carimbo de tempo (padrão: DigiCert).
param([Parameter(Mandatory = $true)][string]$File)
$ErrorActionPreference = 'Stop'

$pfx = $env:TABDISPLAY_SIGN_PFX
$thumb = $env:TABDISPLAY_SIGN_THUMBPRINT
if (-not $pfx -and -not $thumb) {
    Write-Warning "sign-windows: sem certificado configurado, $File fica sem assinatura."
    exit 0
}

$signtool = (Get-Command signtool -ErrorAction SilentlyContinue).Source
if (-not $signtool) {
    $signtool = Get-ChildItem "${env:ProgramFiles(x86)}\Windows Kits\10\bin\*\x64\signtool.exe" -ErrorAction SilentlyContinue |
        Sort-Object FullName -Descending | Select-Object -First 1 -ExpandProperty FullName
}
if (-not $signtool) { throw 'signtool.exe não encontrado (instale o Windows SDK)' }

$timestamp = if ($env:TABDISPLAY_SIGN_TIMESTAMP) { $env:TABDISPLAY_SIGN_TIMESTAMP } else { 'http://timestamp.digicert.com' }
$args = @('sign', '/fd', 'sha256', '/tr', $timestamp, '/td', 'sha256', '/d', 'TabDisplay')
if ($pfx) { $args += @('/f', $pfx, '/p', $env:TABDISPLAY_SIGN_PFX_PASSWORD) } else { $args += @('/sha1', $thumb) }
& $signtool @args $File
if ($LASTEXITCODE) { throw "signtool falhou ($LASTEXITCODE) em $File" }
