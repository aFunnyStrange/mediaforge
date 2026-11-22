param([string]$Destination = (Join-Path (Split-Path $PSScriptRoot) 'target\ffmpeg'))
$ErrorActionPreference = 'Stop'
$url = 'https://github.com/GyanD/codexffmpeg/releases/download/9.0/ffmpeg-9.0-full_build.zip'
$expected = 'F42F0C4B04EAE3AC918707FF66E3E0FF0CEE527BFA6D322624D4BC1160D5055E'
New-Item -ItemType Directory -Force -Path $Destination | Out-Null
$archive = Join-Path $Destination 'ffmpeg-9.0-full_build.zip'
if (-not (Test-Path -LiteralPath $archive) -or (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) {
    Invoke-WebRequest -Uri $url -OutFile $archive -TimeoutSec 600
}
if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $expected) { throw 'FFmpeg SHA-256 mismatch' }
Expand-Archive -LiteralPath $archive -DestinationPath $Destination -Force
$exe = Join-Path $Destination 'ffmpeg-9.0-full_build\bin\ffmpeg.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'FFmpeg archive layout changed' }
Write-Output (Resolve-Path -LiteralPath $exe).Path
