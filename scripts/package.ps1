param(
    [string]$FfmpegPath,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path $PSScriptRoot
Set-Location -LiteralPath $projectRoot
if (-not $FfmpegPath) { $FfmpegPath = (Get-Command ffmpeg -ErrorAction Stop).Source }
$FfmpegPath = (Resolve-Path -LiteralPath $FfmpegPath).Path
$engineRoot = Split-Path (Split-Path $FfmpegPath)
foreach ($required in @('LICENSE', 'README.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $engineRoot $required))) {
        throw "FFmpeg distribution must include $required next to its bin folder. Use the complete Gyan Windows distribution."
    }
}
if (-not $SkipBuild) {
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Rust release build failed' }
}
$bundle = Join-Path $projectRoot 'dist\MediaForge-windows-x64'
if (Test-Path -LiteralPath $bundle) {
    $resolvedBundle = (Resolve-Path -LiteralPath $bundle).Path
    $expectedBundle = [IO.Path]::GetFullPath((Join-Path $projectRoot 'dist\MediaForge-windows-x64'))
    if ($resolvedBundle -ne $expectedBundle) { throw 'Unexpected bundle cleanup path' }
    Remove-Item -LiteralPath $resolvedBundle -Recurse -Force
}
New-Item -ItemType Directory -Force -Path "$bundle\engines", "$bundle\licenses" | Out-Null
Copy-Item -LiteralPath 'target\release\mediaforge.exe' -Destination "$bundle\MediaForge.exe" -Force
Copy-Item -LiteralPath $FfmpegPath -Destination "$bundle\engines\ffmpeg.exe" -Force
Copy-Item -LiteralPath "$engineRoot\LICENSE" -Destination "$bundle\licenses\FFmpeg-LICENSE.txt" -Force
Copy-Item -LiteralPath "$engineRoot\README.txt" -Destination "$bundle\licenses\FFmpeg-README.txt" -Force
Copy-Item -LiteralPath 'README.md' -Destination "$bundle\README.zh-CN.md" -Force
Copy-Item -LiteralPath 'docs\THIRD_PARTY.md' -Destination "$bundle\licenses\THIRD_PARTY.md" -Force
Copy-Item -LiteralPath 'LICENSE' -Destination "$bundle\licenses\MediaForge-LICENSE.txt" -Force
$metadata = cargo metadata --format-version 1 --locked | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'Cannot resolve dependency licenses' }
$dependencies = $metadata.packages | Where-Object { $_.name -ne 'mediaforge' }
$dependencies | Select-Object name,version,license,repository | ConvertTo-Json -Depth 4 | Set-Content -Encoding UTF8 "$bundle\licenses\rust-dependencies.json"
foreach ($dependency in $dependencies) {
    $source = Split-Path $dependency.manifest_path
    $destination = Join-Path "$bundle\licenses\rust" ($dependency.name + '-' + $dependency.version)
    New-Item -ItemType Directory -Force -Path $destination | Out-Null
    Get-ChildItem -LiteralPath $source -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE)' } | Copy-Item -Destination $destination -Force
}
& $FfmpegPath -version | Set-Content -Encoding UTF8 "$bundle\licenses\FFmpeg-version.txt"
Get-FileHash -Algorithm SHA256 "$bundle\MediaForge.exe", "$bundle\engines\ffmpeg.exe" | ForEach-Object { '{0}  {1}' -f $_.Hash, (Split-Path $_.Path -Leaf) } | Set-Content "$bundle\SHA256SUMS.txt"
# Crate archives can contain Unix-epoch timestamps; ZIP requires 1980 or later.
Get-ChildItem -LiteralPath $bundle -Recurse -File | Where-Object { $_.LastWriteTime.Year -lt 1980 } | ForEach-Object { $_.LastWriteTime = [datetime]'1980-01-01T00:00:00' }
Compress-Archive -Path $bundle -DestinationPath 'dist\MediaForge-windows-x64.zip' -Force
Write-Host "Portable application: $bundle\MediaForge.exe"
