# Junta o que o instalador do Windows embute em desktop/src-tauri/resources (fora do git):
#   adb     -> platform-tools no PATH        (winget install Google.PlatformTools)
#   driver  -> Virtual Display Driver 25.7   (winget install VirtualDrivers.Virtual-Display-Driver)
#   APK     -> build release do app Android
# Depois: cd desktop; npx tauri build
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$res = Join-Path $root 'desktop\src-tauri\resources'
New-Item -ItemType Directory -Force (Join-Path $res 'adb'), (Join-Path $res 'vdd') | Out-Null

$adb = Split-Path (Get-Command adb).Source
Copy-Item (Join-Path $adb 'adb.exe'), (Join-Path $adb 'AdbWinApi.dll'), (Join-Path $adb 'AdbWinUsbApi.dll'), (Join-Path $adb 'NOTICE.txt') (Join-Path $res 'adb')

# A pasta se chama x86, mas o driver é x64 (INF NTamd64).
$vdd = Get-ChildItem (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages') -Directory -Filter 'VirtualDrivers.Virtual-Display-Driver_*' |
    ForEach-Object { Join-Path $_.FullName 'SignedDrivers\x86\VDD' } | Select-Object -First 1
Copy-Item (Join-Path $vdd 'MttVDD.inf'), (Join-Path $vdd 'MttVDD.dll'), (Join-Path $vdd 'mttvdd.cat') (Join-Path $res 'vdd')
# Modelo enxuto (poucos modos): com ~100+ modos o driver não cria o monitor.
Copy-Item (Join-Path $root 'desktop\src-tauri\windows\vdd_settings.xml') (Join-Path $res 'vdd')

Push-Location (Join-Path $root 'android')
try { & .\gradlew.bat assembleRelease --console=plain -q; if ($LASTEXITCODE) { throw 'gradle falhou' } } finally { Pop-Location }
Copy-Item (Join-Path $root 'android\app\build\outputs\apk\release\app-release.apk') (Join-Path $res 'tabdisplay.apk')

Get-ChildItem $res -Recurse -File | Select-Object @{n = 'arquivo'; e = { $_.FullName.Substring($res.Length + 1) } }, Length
