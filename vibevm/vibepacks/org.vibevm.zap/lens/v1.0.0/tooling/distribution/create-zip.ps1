$ErrorActionPreference = 'Stop'
$source = [IO.Path]::GetFullPath($env:ZAP_DISTRIBUTION_SOURCE)
$destination = [IO.Path]::GetFullPath($env:ZAP_DISTRIBUTION_ZIP)
if (-not (Test-Path -LiteralPath $source -PathType Container)) {
    throw 'distribution source directory is unavailable'
}
if (Test-Path -LiteralPath $destination) {
    throw 'distribution ZIP destination already exists'
}
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = [IO.Compression.ZipFile]::Open(
    $destination,
    [IO.Compression.ZipArchiveMode]::Create
)
try {
    $files = Get-ChildItem -LiteralPath $source -Recurse -Force -File |
        Sort-Object -Property FullName
    foreach ($file in $files) {
        if (($file.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) {
            throw "distribution ZIP refuses reparse point"
        }
        $relative = $file.FullName.Substring($source.Length).TrimStart([char[]]'\/')
        $entry = $relative.Replace('\', '/')
        [IO.Compression.ZipFileExtensions]::CreateEntryFromFile(
            $archive,
            $file.FullName,
            $entry,
            [IO.Compression.CompressionLevel]::Optimal
        ) | Out-Null
    }
}
finally {
    $archive.Dispose()
}
