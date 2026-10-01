$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$source = Join-Path $projectRoot 'edge-extension'
$stage = Join-Path $projectRoot 'work\edge-store-package'
$outputs = Join-Path $projectRoot 'outputs'
New-Item -ItemType Directory -Force -Path $stage,$outputs | Out-Null
# Only known runtime files go into the store ZIP.
foreach ($name in @('service_worker.js','popup.html','popup.js','popup.css')) {
    Copy-Item -LiteralPath (Join-Path $source $name) -Destination $stage -Force
}
$manifest = Get-Content -LiteralPath (Join-Path $source 'manifest.json') -Raw -Encoding utf8 | ConvertFrom-Json
$manifest.PSObject.Properties.Remove('key')
$manifest | Add-Member -MemberType NoteProperty -Name icons -Value @{ '16'='icon16.png'; '48'='icon48.png'; '128'='icon128.png' } -Force
$manifest | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $stage 'manifest.json') -Encoding utf8

# Code-drawn packaging asset; no remote resource or external image service.
Add-Type -AssemblyName System.Drawing
foreach ($size in @(16,48,128)) {
    $bitmap = New-Object System.Drawing.Bitmap($size,$size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::FromArgb(37,99,235))
    $brush = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::White)
    $graphics.FillRectangle($brush,[single]($size*.23),[single]($size*.2),[single]($size*.54),[single]($size*.62))
    $triangle = [System.Drawing.PointF[]]@(
        [System.Drawing.PointF]::new([single]($size*.23),[single]($size*.82)),
        [System.Drawing.PointF]::new([single]($size*.5),[single]($size*.64)),
        [System.Drawing.PointF]::new([single]($size*.77),[single]($size*.82)))
    $cutout = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(37,99,235))
    $graphics.FillPolygon($cutout,$triangle)
    $bitmap.Save((Join-Path $stage "icon$size.png"),[System.Drawing.Imaging.ImageFormat]::Png)
    $cutout.Dispose(); $brush.Dispose(); $graphics.Dispose(); $bitmap.Dispose()
}
$runtime = @('manifest.json','service_worker.js','popup.html','popup.js','popup.css','icon16.png','icon48.png','icon128.png') | ForEach-Object { Join-Path $stage $_ }
Compress-Archive -LiteralPath $runtime -DestinationPath (Join-Path $outputs 'favorite-launcher-edge-store.zip') -Force
Compress-Archive -Path (Join-Path $source '*') -DestinationPath (Join-Path $outputs 'search-launcher-edge-extension.zip') -Force
Write-Host 'Extension packages created in outputs.'
