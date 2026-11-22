param([string]$Archive = (Join-Path (Split-Path $PSScriptRoot) 'dist\MediaForge-windows-x64.zip'))
$ErrorActionPreference = 'Stop'
$destination = Join-Path ([IO.Path]::GetTempPath()) ('mediaforge-check-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $destination | Out-Null
$process = $null
try {
    Expand-Archive -LiteralPath $Archive -DestinationPath $destination
    $bundle = Join-Path $destination 'MediaForge-windows-x64'
    foreach ($line in Get-Content -LiteralPath "$bundle\SHA256SUMS.txt") {
        $hash, $name = $line -split '\s+', 2
        $relative = if ($name -eq 'ffmpeg.exe') { 'engines\ffmpeg.exe' } elseif ($name -eq 'MediaForge.exe') { $name } else { throw "Unexpected checksum entry: $name" }
        if ((Get-FileHash -LiteralPath (Join-Path $bundle $relative) -Algorithm SHA256).Hash -ne $hash) { throw "Package checksum mismatch: $name" }
    }
    foreach ($file in @('README.zh-CN.md', 'licenses\FFmpeg-LICENSE.txt', 'licenses\rust-dependencies.json')) {
        if (-not (Test-Path -LiteralPath (Join-Path $bundle $file))) { throw "Missing package file: $file" }
    }
    # Actual offline conversion by the extracted engine, independent of a system FFmpeg.
    & "$bundle\engines\ffmpeg.exe" -hide_banner -loglevel error -f lavfi -i 'sine=duration=0.2' -c:a libmp3lame "$destination\audio.mp3"
    if ($LASTEXITCODE -ne 0) { throw 'Packaged engine conversion failed' }
    & "$bundle\engines\ffmpeg.exe" -hide_banner -loglevel error -i "$destination\audio.mp3" -f null -
    if ($LASTEXITCODE -ne 0) { throw 'Packaged output cannot be decoded' }
    $bytes = [IO.File]::ReadAllBytes("$bundle\MediaForge.exe")
    $pe = [BitConverter]::ToInt32($bytes, 0x3c)
    if ([BitConverter]::ToUInt16($bytes, $pe + 24 + 68) -ne 2) { throw 'Application is not a Windows GUI executable' }
    $process = Start-Process -FilePath "$bundle\MediaForge.exe" -WorkingDirectory $destination -WindowStyle Hidden -PassThru
    Start-Sleep -Seconds 2
    if ($process.HasExited) { throw "GUI exited during startup: $($process.ExitCode)" }
    Write-Output 'Verified extracted ZIP, checksums, offline conversion, GUI subsystem and startup.'
} finally {
    if ($process -and -not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
    $resolved = (Resolve-Path -LiteralPath $destination).Path
    if ($resolved -ne [IO.Path]::GetFullPath($destination) -or -not $resolved.StartsWith([IO.Path]::GetFullPath([IO.Path]::GetTempPath()), [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected cleanup path' }
    Remove-Item -LiteralPath $resolved -Recurse -Force
}
