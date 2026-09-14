[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$SourceRoot,
    [Parameter(Mandatory = $true)]
    [string]$RegistryRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$source = [System.IO.Path]::GetFullPath($SourceRoot)
$registry = [System.IO.Path]::GetFullPath($RegistryRoot)
$packageVersion = '1.1.0'
$manifestPath = Join-Path $registry 'ZAP-PAYLOAD-MANIFEST.json'
$final = Join-Path $registry "org.vibevm.world/zap/v$packageVersion"
$staging = Join-Path $registry ('.zap-stage-' + [guid]::NewGuid().ToString('N'))

if (-not (Test-Path -LiteralPath (Join-Path $source 'vibe.toml') -PathType Leaf)) {
    throw 'SourceRoot is not the ZAP package root.'
}
$declaredVibeVersion = Get-Content -LiteralPath (Join-Path $source 'vibe.toml') |
    Where-Object { $_ -match '^version\s*=\s*"([^"]+)"\s*$' } |
    Select-Object -First 1
$declaredCargoVersion = Get-Content -LiteralPath (Join-Path $source 'Cargo.toml') |
    Where-Object { $_ -match '^version\s*=\s*"([^"]+)"\s*$' } |
    Select-Object -First 1
if ($declaredVibeVersion -notmatch '^version\s*=\s*"1\.1\.0"\s*$' -or
    $declaredCargoVersion -notmatch '^version\s*=\s*"1\.1\.0"\s*$') {
    throw "Source package versions do not match the assembler target $packageVersion."
}
if (Test-Path -LiteralPath $final) {
    throw "Refusing to replace existing artifact payload: $final"
}
if (Test-Path -LiteralPath $manifestPath) {
    throw "Refusing to replace existing artifact manifest: $manifestPath"
}

$rootFiles = @(
    '.gitattributes', '.vibeignore', 'Cargo.lock', 'Cargo.toml',
    'conform-baseline.json', 'conform.toml', 'LICENSE.md',
    'README.md', 'specmap.json', 'specmap.toml', 'vibe.toml'
)
$rootDirectories = @('crates', 'discipline', 'vibevm/vibespecs')

$supportedIgnore = @(
    'vibevm/vibespecs/development/**',
    'vibevm/vibespecs/skills/zap-state/scripts/**',
    'vibevm/vibespecs/skills/zap-run/scripts/**',
    'vibevm/vibespecs/examples/zap/**',
    'vibevm/vibespecs/flows/zap/ZAP-PYTHON-API.md',
    'vibevm/vibespecs/research/zap/OWNER-*.txt',
    'vibevm/vibespecs/research/zap/ZAP-STRATEGIC-MAP-FOLLOWUP-*.md',
    'vibevm/vibedeps/**',
    'vibevm/vibespecs/boot/.vibe-boot-artifacts.lock',
    'vibevm/vibespecs/boot/INDEX.md',
    'vibevm/vibespecs/boot/STATIC.md',
    'vibevm/vibespecs/boot/STATIC.xml',
    'AGENTS.md', 'CLAUDE.md', 'GEMINI.md',
    'vibe.lock', '**/target/**', '.agents/**',
    '.claude/**', '.codex/**', '.opencode/**', '.vibe/**',
    '**/__pycache__/**', '**/*.pyc', '**/*.token', '**/trust.json',
    'private/**'
) | Sort-Object
$actualIgnore = Get-Content -LiteralPath (Join-Path $source '.vibeignore') |
    ForEach-Object { $_.Trim() } |
    Where-Object { $_ -and -not $_.StartsWith('#') } |
    Sort-Object
if (($supportedIgnore -join "`n") -ne ($actualIgnore -join "`n")) {
    throw 'The package ignore contract changed; update this explicit assembler before staging.'
}

function Get-RelativePath([string]$path) {
    return [System.IO.Path]::GetRelativePath($source, $path).Replace('\', '/')
}

function Test-TargetPathSegment([string]$relative) {
    $lower = $relative.ToLowerInvariant()
    return $lower -eq 'target' -or $lower.StartsWith('target/') -or
        $lower.EndsWith('/target') -or $lower.Contains('/target/')
}

function Test-Excluded([string]$relative) {
    $lower = $relative.ToLowerInvariant()
    if (Test-TargetPathSegment $relative) { return $true }
    if ($lower -in @(
        'vibe.lock', 'trust.json', 'agents.md', 'claude.md', 'gemini.md',
        'vibevm/vibespecs/boot/.vibe-boot-artifacts.lock',
        'vibevm/vibespecs/boot/index.md',
        'vibevm/vibespecs/boot/static.md',
        'vibevm/vibespecs/boot/static.xml'
    )) { return $true }
    foreach ($prefix in @(
        'vibevm/vibespecs/development/',
        'vibevm/vibespecs/skills/zap-state/scripts/',
        'vibevm/vibespecs/skills/zap-run/scripts/',
        'vibevm/vibespecs/examples/zap/',
        'vibevm/vibedeps/', '.agents/', '.claude/',
        '.codex/', '.opencode/', '.vibe/', 'private/'
    )) {
        if ($lower.StartsWith($prefix)) { return $true }
    }
    if ($lower -eq 'vibevm/vibespecs/flows/zap/zap-python-api.md') { return $true }
    if ($lower.Contains('/__pycache__/') -or $lower.EndsWith('.pyc') -or $lower.EndsWith('.token')) {
        return $true
    }
    if ($lower.EndsWith('/trust.json')) { return $true }
    if ($lower -like 'vibevm/vibespecs/research/zap/owner-*.txt') { return $true }
    if ($lower -like 'vibevm/vibespecs/research/zap/zap-strategic-map-followup-*.md') { return $true }
    return $false
}

function Get-SourceRows {
    $paths = [System.Collections.Generic.List[string]]::new()
    foreach ($file in $rootFiles) {
        $path = Join-Path $source $file
        if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {
            throw "Required package file is unavailable: $file"
        }
        $paths.Add([System.IO.Path]::GetFullPath($path))
    }
    foreach ($directory in $rootDirectories) {
        $path = Join-Path $source $directory
        if (-not (Test-Path -LiteralPath $path -PathType Container)) {
            throw "Required package directory is unavailable: $directory"
        }
        foreach ($file in Get-ChildItem -LiteralPath $path -Recurse -File) {
            $paths.Add($file.FullName)
        }
    }

    $rows = foreach ($path in $paths | Sort-Object -Unique) {
        $relative = Get-RelativePath $path
        if (-not (Test-Excluded $relative)) {
            $item = Get-Item -LiteralPath $path
            [pscustomobject][ordered]@{
                path = $relative
                bytes = [uint64]$item.Length
                sha256 = (Get-FileHash -LiteralPath $path -Algorithm SHA256).Hash.ToLowerInvariant()
            }
        }
    }
    return @($rows | Sort-Object path)
}

$before = Get-SourceRows
if ($before.Count -eq 0) { throw 'The explicit package selection is empty.' }

New-Item -ItemType Directory -Path $staging -Force | Out-Null
foreach ($row in $before) {
    $sourcePath = Join-Path $source $row.path
    $destinationPath = Join-Path $staging $row.path
    $parent = Split-Path -Parent $destinationPath
    New-Item -ItemType Directory -Path $parent -Force | Out-Null
    [System.IO.File]::Copy($sourcePath, $destinationPath, $false)
}

$after = Get-SourceRows
if (($before | ConvertTo-Json -Depth 4 -Compress) -ne ($after | ConvertTo-Json -Depth 4 -Compress)) {
    throw "Source bytes changed during assembly; incomplete staging remains at $staging"
}

$destinationRows = foreach ($file in Get-ChildItem -LiteralPath $staging -Recurse -File) {
    [pscustomobject][ordered]@{
        path = [System.IO.Path]::GetRelativePath($staging, $file.FullName).Replace('\', '/')
        bytes = [uint64]$file.Length
        sha256 = (Get-FileHash -LiteralPath $file.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
    }
}
$destinationRows = @($destinationRows | Sort-Object path)
if (($before | ConvertTo-Json -Depth 4 -Compress) -ne ($destinationRows | ConvertTo-Json -Depth 4 -Compress)) {
    throw "Staged bytes differ from the captured source; incomplete staging remains at $staging"
}

$forbidden = @($destinationRows | Where-Object {
    $_.path.ToLowerInvariant().EndsWith('.py') -or
    $_.path.ToLowerInvariant().EndsWith('.pyc') -or
    (Test-TargetPathSegment $_.path) -or
    $_.path.ToLowerInvariant().Contains('/development/') -or
    $_.path.ToLowerInvariant().StartsWith('vibevm/vibedeps/') -or
    $_.path.ToLowerInvariant().StartsWith('.agents/') -or
    $_.path.ToLowerInvariant().StartsWith('.claude/') -or
    $_.path.ToLowerInvariant().StartsWith('.codex/') -or
    $_.path.ToLowerInvariant().StartsWith('.opencode/') -or
    $_.path.ToLowerInvariant().StartsWith('.vibe/') -or
    $_.path.ToLowerInvariant() -eq 'vibe.lock' -or
    $_.path.ToLowerInvariant().EndsWith('.token') -or
    $_.path.ToLowerInvariant().EndsWith('/trust.json')
})
if ($forbidden.Count -ne 0) {
    throw "Forbidden payload material was staged: $($forbidden[0].path)"
}

$rowsJson = $destinationRows | ConvertTo-Json -Depth 4 -Compress
$rowsBytes = [System.Text.Encoding]::UTF8.GetBytes($rowsJson)
$payloadDigest = [Convert]::ToHexString([System.Security.Cryptography.SHA256]::HashData($rowsBytes)).ToLowerInvariant()

$finalParent = Split-Path -Parent $final
New-Item -ItemType Directory -Path $finalParent -Force | Out-Null
Move-Item -LiteralPath $staging -Destination $final

$manifest = [pscustomobject][ordered]@{
    schema = 'zap-package-source-manifest/1'
    generated_at_utc = (Get-Date).ToUniversalTime().ToString('o')
    package = "flow:org.vibevm.world/zap@=$packageVersion"
    version = $packageVersion
    source_root = $source
    payload_root = $final
    selection = [pscustomobject][ordered]@{
        root_files = $rootFiles
        root_directories = $rootDirectories
        exclusions = $actualIgnore
    }
    file_count = $destinationRows.Count
    payload_manifest_sha256 = $payloadDigest
    files = $destinationRows
}
$manifest | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $manifestPath -Encoding utf8
Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json | Out-Null

[pscustomobject][ordered]@{
    payload_root = $final
    manifest = $manifestPath
    file_count = $destinationRows.Count
    payload_manifest_sha256 = $payloadDigest
} | ConvertTo-Json -Depth 4
