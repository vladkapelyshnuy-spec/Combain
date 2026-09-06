[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$FfmpegVersion = "8.0.1"
$TargetTriple = "x86_64-pc-windows-msvc"
$ArchiveUrl = "https://github.com/GyanD/codexffmpeg/releases/download/8.0.1/ffmpeg-8.0.1-full_build.zip"
$ArchiveSha256 = "467CDE100A47ED4B03A897988AEB4A296890C1E2B2D2864204657D002BC5FB90"

$RepositoryRoot = Split-Path -Parent $PSScriptRoot
$BinaryDirectory = Join-Path $RepositoryRoot "src-tauri\binaries"
$FfmpegDestination = Join-Path $BinaryDirectory "ffmpeg-$TargetTriple.exe"
$FfprobeDestination = Join-Path $BinaryDirectory "ffprobe-$TargetTriple.exe"
$ManifestPath = Join-Path $BinaryDirectory "ffmpeg-sidecars.json"

function Get-Sha256 {
    param([Parameter(Mandatory)][string]$Path)

    return (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToUpperInvariant()
}

function Test-ExistingSidecars {
    if (-not (Test-Path -LiteralPath $FfmpegDestination -PathType Leaf) -or
        -not (Test-Path -LiteralPath $FfprobeDestination -PathType Leaf) -or
        -not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        return $false
    }

    try {
        $manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
        return $manifest.version -eq $FfmpegVersion -and
            $manifest.targetTriple -eq $TargetTriple -and
            $manifest.archiveUrl -eq $ArchiveUrl -and
            $manifest.archiveSha256 -eq $ArchiveSha256 -and
            (Get-Sha256 -Path $FfmpegDestination) -eq $manifest.ffmpegSha256 -and
            (Get-Sha256 -Path $FfprobeDestination) -eq $manifest.ffprobeSha256
    }
    catch {
        return $false
    }
}

function Assert-Executable {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][ValidateSet("ffmpeg", "ffprobe")][string]$Product
    )

    $output = & $Path -version 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "$Name failed its '-version' check with exit code $LASTEXITCODE."
    }

    $versionLine = ($output | Select-Object -First 1) -as [string]
    $expectedVersionPattern = "^$Product version $([regex]::Escape($FfmpegVersion))"
    if (-not $versionLine -or $versionLine -notmatch $expectedVersionPattern) {
        throw "$Name did not report the expected FFmpeg version $FfmpegVersion."
    }
}

if (Test-ExistingSidecars) {
    Write-Host "Bundled FFmpeg $FfmpegVersion sidecars already match the verified manifest."
    exit 0
}

New-Item -ItemType Directory -Path $BinaryDirectory -Force | Out-Null
$TemporaryDirectory = Join-Path ([System.IO.Path]::GetTempPath()) ("combain-ffmpeg-" + [guid]::NewGuid().ToString("N"))
$ArchivePath = Join-Path $TemporaryDirectory "ffmpeg-$FfmpegVersion-full_build.zip"
$ExtractionDirectory = Join-Path $TemporaryDirectory "extracted"

try {
    New-Item -ItemType Directory -Path $ExtractionDirectory -Force | Out-Null

    Write-Host "Downloading pinned FFmpeg $FfmpegVersion archive..."
    Invoke-WebRequest -Uri $ArchiveUrl -OutFile $ArchivePath -UseBasicParsing

    $downloadedHash = Get-Sha256 -Path $ArchivePath
    if ($downloadedHash -ne $ArchiveSha256) {
        throw "FFmpeg archive SHA-256 mismatch. Expected $ArchiveSha256, got $downloadedHash."
    }

    Expand-Archive -LiteralPath $ArchivePath -DestinationPath $ExtractionDirectory -Force

    $ffmpegSource = @(Get-ChildItem -LiteralPath $ExtractionDirectory -Recurse -File -Filter "ffmpeg.exe")
    $ffprobeSource = @(Get-ChildItem -LiteralPath $ExtractionDirectory -Recurse -File -Filter "ffprobe.exe")

    if ($ffmpegSource.Count -eq 0) {
        throw "The verified archive does not contain ffmpeg.exe."
    }
    if ($ffmpegSource.Count -gt 1) {
        throw "The verified archive contains multiple ffmpeg.exe files; refusing an ambiguous installation."
    }
    if ($ffprobeSource.Count -eq 0) {
        throw "The verified archive does not contain ffprobe.exe."
    }
    if ($ffprobeSource.Count -gt 1) {
        throw "The verified archive contains multiple ffprobe.exe files; refusing an ambiguous installation."
    }

    $stagedFfmpeg = Join-Path $TemporaryDirectory "ffmpeg-$TargetTriple.exe"
    $stagedFfprobe = Join-Path $TemporaryDirectory "ffprobe-$TargetTriple.exe"
    Copy-Item -LiteralPath $ffmpegSource.FullName -Destination $stagedFfmpeg
    Copy-Item -LiteralPath $ffprobeSource.FullName -Destination $stagedFfprobe

    Assert-Executable -Path $stagedFfmpeg -Name "ffmpeg" -Product "ffmpeg"
    Assert-Executable -Path $stagedFfprobe -Name "ffprobe" -Product "ffprobe"

    Copy-Item -LiteralPath $stagedFfmpeg -Destination $FfmpegDestination -Force
    Copy-Item -LiteralPath $stagedFfprobe -Destination $FfprobeDestination -Force

    Assert-Executable -Path $FfmpegDestination -Name "copied ffmpeg sidecar" -Product "ffmpeg"
    Assert-Executable -Path $FfprobeDestination -Name "copied ffprobe sidecar" -Product "ffprobe"

    $manifest = [ordered]@{
        version        = $FfmpegVersion
        targetTriple   = $TargetTriple
        archiveUrl     = $ArchiveUrl
        archiveSha256  = $ArchiveSha256
        ffmpegSha256   = Get-Sha256 -Path $FfmpegDestination
        ffprobeSha256  = Get-Sha256 -Path $FfprobeDestination
    }
    $manifest | ConvertTo-Json | Set-Content -LiteralPath $ManifestPath -Encoding UTF8

    Write-Host "Bundled FFmpeg sidecars are ready in $BinaryDirectory"
}
finally {
    if (Test-Path -LiteralPath $TemporaryDirectory) {
        Remove-Item -LiteralPath $TemporaryDirectory -Recurse -Force
    }
}
