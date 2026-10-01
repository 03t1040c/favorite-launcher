$ErrorActionPreference = 'Stop'
$projectRoot = Split-Path -Parent $PSScriptRoot
$version = (Get-Content -LiteralPath (Join-Path $projectRoot 'package.json') -Raw -Encoding utf8 | ConvertFrom-Json).version
Push-Location $projectRoot
try {
    & npm run tauri build -- --bundles nsis,msi
    if ($LASTEXITCODE -ne 0) { throw 'Tauri release build failed.' }
    $release = Join-Path $projectRoot 'work\target\release'
    $outputs = Join-Path $projectRoot 'outputs'
    New-Item -ItemType Directory -Force -Path $outputs | Out-Null
    foreach ($file in @('search-launcher-app.exe',"bundle\nsis\search-launcher-app_${version}_x64-setup.exe","bundle\msi\search-launcher-app_${version}_x64_en-US.msi")) {
        Copy-Item -LiteralPath (Join-Path $release $file) -Destination $outputs -Force
    }
    & powershell -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'package-extension.ps1')
    if ($LASTEXITCODE -ne 0) { throw 'Extension packaging failed.' }
} finally { Pop-Location }
