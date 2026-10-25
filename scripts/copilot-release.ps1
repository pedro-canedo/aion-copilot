# aion-copilot: prepares a release (see "Publicar uma versão" in COPILOT.md).
#
# Runs the tests, builds the offline installer, writes its .sha256 and the
# unversioned copy the site links to, then PRINTS the git and gh commands to
# publish it. It never tags, pushes or publishes anything itself.
#
#   powershell -ExecutionPolicy Bypass -File scripts\copilot-release.ps1
#   ... -SkipTests     (tests already run)
#   ... -SkipBuild     (installer already built; only hash and copy)

param(
    [switch]$SkipTests,
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

function Fail($message) {
    Write-Host "ERRO: $message" -ForegroundColor Red
    exit 1
}

function Run($what, [scriptblock]$command) {
    Write-Host "==> $what" -ForegroundColor Cyan
    & $command
    if ($LASTEXITCODE -ne 0) { Fail "$what falhou (código $LASTEXITCODE)" }
}

# 1. The version comes only from tauri.conf.json, and the changelog must
#    open with it (Cargo.toml and package.json keep upstream's version).
$conf = Get-Content -Raw -Encoding UTF8 'src-tauri\tauri.conf.json' | ConvertFrom-Json
$version = $conf.version
if ($version -notmatch '^\d+\.\d+\.\d+$') { Fail "versão inválida no tauri.conf.json: $version" }
$changelog = Get-Content -Raw -Encoding UTF8 'src\data\copilot_changelog.json' | ConvertFrom-Json
$entry = $changelog.versions[0]
if ($entry.version -ne $version) {
    Fail "a primeira entrada de src\data\copilot_changelog.json é $($entry.version), não $version"
}
if (-not (Select-String -Quiet -SimpleMatch "Windows · v$version<" 'site\index.html' -Encoding UTF8)) {
    Write-Host "Aviso: o selo de versão em site\index.html não diz v$version." -ForegroundColor Yellow
}
$tag = "copilot-v$version"
if (git tag --list $tag) { Fail "a tag $tag já existe" }
if (git status --porcelain) {
    Write-Host "Aviso: há mudanças não commitadas; a tag vai apontar para o último commit." -ForegroundColor Yellow
}
Write-Host "aion-copilot $version ($tag)" -ForegroundColor Green

# 2. Tests and the installer.
if (-not $SkipTests) {
    Push-Location 'src-tauri'
    try {
        Run 'testes Rust' { cargo test --no-default-features --features desktop,obs,selfupdate }
    } finally {
        Pop-Location
    }
    Run 'testes JS' { node --test (Get-ChildItem tests -Filter '*.test.mjs' | ForEach-Object FullName) }
}
if (-not $SkipBuild) {
    Run 'npm run build:offline' { npm run build:offline }
}

$bundle = Join-Path $root 'src-tauri\target\release\bundle\nsis'
$exeName = "aion-copilot_${version}_x64-setup.exe"
$exe = Join-Path $bundle $exeName
if (-not (Test-Path $exe)) { Fail "instalador não encontrado: $exe" }

# 3. "<hex>  <file>", lowercase, ASCII without a BOM: the app reads the first
#    word and wants exactly 64 hex digits.
$hash = (Get-FileHash -Algorithm SHA256 $exe).Hash.ToLowerInvariant()
$sha = "$exe.sha256"
[System.IO.File]::WriteAllText($sha, "$hash  $exeName`n", [System.Text.Encoding]::ASCII)

# 4. The unversioned copy behind releases/latest/download/aion-copilot_x64-setup.exe.
$latest = Join-Path $bundle 'aion-copilot_x64-setup.exe'
Copy-Item -Force $exe $latest

# 5. Release notes from the changelog entry.
$labels = @{ novo = 'Novo'; melhorado = 'Melhorado'; corrigido = 'Corrigido' }
$notes = Join-Path $bundle "notes-$version.md"
$lines = @("## $($entry.title)", '')
foreach ($item in $entry.items) { $lines += "- **$($labels[$item.tag])**: $($item.text)" }
$lines += @('', "SHA-256 de ``$exeName``: ``$hash``")
[System.IO.File]::WriteAllText($notes, ($lines -join "`n") + "`n", (New-Object System.Text.UTF8Encoding $false))

Write-Host ''
Write-Host "Pronto: $exeName" -ForegroundColor Green
Write-Host "  SHA-256 $hash"
Write-Host ''
Write-Host 'Para publicar, depois de commitar a versão, rode (nunca git push --tags: as tags v2.0.x são do upstream):' -ForegroundColor Cyan
Write-Host "  git tag $tag"
Write-Host "  git push origin $tag"
Write-Host "  gh release create $tag --verify-tag --title `"aion-copilot $version`" --notes-file `"$notes`" `"$exe`" `"$sha`" `"$latest`""
Write-Host '  git push origin main   # só depois do release: o site e o README apontam para ele'
