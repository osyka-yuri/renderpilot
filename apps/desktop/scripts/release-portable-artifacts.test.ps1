[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "release-helpers.ps1")

function Assert-RenderPilotEqual {
    param(
        [Parameter(Mandatory)] $Actual,
        [Parameter(Mandatory)] $Expected,
        [Parameter(Mandatory)] [string] $Description
    )

    if ($Actual -cne $Expected) {
        throw "$Description. Expected '$Expected', got '$Actual'."
    }
}

function Assert-RenderPilotTrue {
    param(
        [Parameter(Mandatory)] [bool] $Condition,
        [Parameter(Mandatory)] [string] $Description
    )

    if (-not $Condition) {
        throw $Description
    }
}

function Assert-RenderPilotBytesEqual {
    param(
        [Parameter(Mandatory)] [byte[]] $Actual,
        [Parameter(Mandatory)] [byte[]] $Expected,
        [Parameter(Mandatory)] [string] $Description
    )

    if (
        $Actual.Length -ne $Expected.Length -or
        [Convert]::ToBase64String($Actual) -cne [Convert]::ToBase64String($Expected)
    ) {
        throw $Description
    }
}

function Assert-RenderPilotThrows {
    param(
        [Parameter(Mandatory)] [scriptblock] $Action,
        [Parameter(Mandatory)] [string] $Description
    )

    $threw = $false
    try {
        & $Action
    }
    catch {
        $threw = $true
    }
    if (-not $threw) {
        throw "$Description did not fail closed."
    }
}

$rootName = "renderpilot-portable-artifacts-contract-test-{0}" -f [Guid]::NewGuid().ToString("N")
$temporaryParent = (Resolve-Path -LiteralPath ([IO.Path]::GetTempPath())).Path.TrimEnd(
    [IO.Path]::DirectorySeparatorChar,
    [IO.Path]::AltDirectorySeparatorChar
)
$testRoot = Join-Path $temporaryParent $rootName
$ownedRoot = $null
$resolvedRoot = $null
$knownFiles = [Collections.Generic.List[string]]::new()
$knownDirectories = [Collections.Generic.List[string]]::new()

try {
    $releaseScript = Get-Content -LiteralPath (Join-Path $PSScriptRoot "release-portable-artifacts.ps1") -Raw
    $portableAppBuild = [regex]::Match(
        $releaseScript,
        '(?s)Invoke-RenderPilotWithWindowsManifest\s+-Selector\s+production\s+-Command\s+\{.*?pnpm tauri build --features portable --no-bundle'
    )
    Assert-RenderPilotTrue -Condition $portableAppBuild.Success `
        -Description "portable App build must scope the production Windows manifest selector"
    $supervisorPattern = [regex]::new(
        '(?s)Invoke-RenderPilotWithWindowsManifest\s+-Selector\s+production\s+-Command\s+\{.*?cargo build --locked --package renderpilot-desktop --bin portable-supervisor --features portable --release'
    )
    $supervisorBuild = $supervisorPattern.Match(
        $releaseScript,
        $portableAppBuild.Index + $portableAppBuild.Length
    )
    Assert-RenderPilotTrue -Condition $supervisorBuild.Success `
        -Description "portable supervisor build must scope the production Windows manifest selector"
    Assert-RenderPilotTrue -Condition ($portableAppBuild.Index -lt $supervisorBuild.Index) `
        -Description "portable App manifest selection must complete before production supervisor selection"

    $ownedRoot = (New-Item -ItemType Directory -Path $testRoot -ErrorAction Stop).FullName
    $resolvedRoot = (Resolve-Path -LiteralPath $ownedRoot).Path.TrimEnd(
        [IO.Path]::DirectorySeparatorChar,
        [IO.Path]::AltDirectorySeparatorChar
    )
    Assert-RenderPilotEqual -Actual ([IO.Path]::GetFileName($resolvedRoot)) -Expected $rootName `
        -Description "test root must retain its generated leaf name"
    Assert-RenderPilotEqual -Actual ([IO.Path]::GetDirectoryName($resolvedRoot)) -Expected $temporaryParent `
        -Description "test root must be an exact child of the resolved temporary parent"

    $legacyZipRoot = (New-Item -ItemType Directory -Path (Join-Path $resolvedRoot "renderpilot-portable-zip") -ErrorAction Stop).FullName
    $legacyRenderRoot = (New-Item -ItemType Directory -Path (Join-Path $legacyZipRoot "RenderPilot") -ErrorAction Stop).FullName
    $legacyForeign = Join-Path $legacyRenderRoot "foreign"
    [IO.File]::WriteAllBytes($legacyForeign, [Text.Encoding]::UTF8.GetBytes("foreign legacy state"))
    $knownFiles.Add($legacyForeign)
    $knownDirectories.Add($legacyRenderRoot)
    $knownDirectories.Add($legacyZipRoot)

    $firstUnique = New-RenderPilotUniqueStagingRoot -Parent $resolvedRoot -Prefix "renderpilot-portable-zip-"
    $secondUnique = New-RenderPilotUniqueStagingRoot -Parent $resolvedRoot -Prefix "renderpilot-portable-zip-"
    $firstUnique = (Resolve-Path -LiteralPath $firstUnique).Path
    $secondUnique = (Resolve-Path -LiteralPath $secondUnique).Path
    $knownDirectories.Add($firstUnique)
    $knownDirectories.Add($secondUnique)
    Assert-RenderPilotTrue -Condition ($firstUnique -cne $secondUnique) `
        -Description "unique staging roots must not reuse a predictable directory"
    foreach ($uniqueRoot in @($firstUnique, $secondUnique)) {
        Assert-RenderPilotEqual -Actual ([IO.Path]::GetDirectoryName($uniqueRoot)) -Expected $resolvedRoot `
            -Description "unique staging root must stay beneath the isolated test root"
        Assert-RenderPilotTrue -Condition (Test-Path -LiteralPath $uniqueRoot -PathType Container) `
            -Description "unique staging root must exist"
    }
    Assert-RenderPilotEqual -Actual ([Text.Encoding]::UTF8.GetString([IO.File]::ReadAllBytes($legacyForeign))) `
        -Expected "foreign legacy state" -Description "legacy predictable foreign state must survive"

    $source = Join-Path $resolvedRoot "source.exe"
    $occupied = Join-Path $resolvedRoot "occupied.exe"
    $copied = Join-Path $resolvedRoot "copied.exe"
    $zipPath = Join-Path $resolvedRoot "portable.zip"
    $sourceBytes = [Text.Encoding]::UTF8.GetBytes("Portable artifact exact contract bytes")
    $occupiedBytes = [Text.Encoding]::UTF8.GetBytes("preexisting foreign destination")
    [IO.File]::WriteAllBytes($source, $sourceBytes)
    [IO.File]::WriteAllBytes($occupied, $occupiedBytes)
    $knownFiles.Add($source)
    $knownFiles.Add($occupied)
    $knownFiles.Add($copied)
    $knownFiles.Add($zipPath)

    Assert-RenderPilotThrows -Description "create-new copy over occupied destination" -Action {
        Copy-RenderPilotFileCreateNew -Source $source -Destination $occupied
    }
    Assert-RenderPilotBytesEqual -Actual ([IO.File]::ReadAllBytes($occupied)) -Expected $occupiedBytes `
        -Description "occupied destination bytes must remain unchanged after refused copy"

    Copy-RenderPilotFileCreateNew -Source $source -Destination $copied
    Assert-RenderPilotBytesEqual -Actual ([IO.File]::ReadAllBytes($copied)) -Expected $sourceBytes `
        -Description "create-new copy must retain exact source bytes"

    Assert-RenderPilotThrows -Description "create-new ZIP over occupied destination" -Action {
        New-RenderPilotPortableZip -Source $source -Destination $occupied -EntryName "RenderPilot/renderpilot-desktop.exe"
    }
    Assert-RenderPilotBytesEqual -Actual ([IO.File]::ReadAllBytes($occupied)) -Expected $occupiedBytes `
        -Description "occupied ZIP destination bytes must remain unchanged"

    New-RenderPilotPortableZip -Source $source -Destination $zipPath -EntryName "RenderPilot/renderpilot-desktop.exe"
    Assert-RenderPilotTrue -Condition (Test-Path -LiteralPath $zipPath -PathType Leaf) `
        -Description "portable ZIP must be created at the exact unoccupied destination"

    $zipStream = [IO.File]::Open($zipPath, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $archive = [IO.Compression.ZipArchive]::new($zipStream, [IO.Compression.ZipArchiveMode]::Read, $false)
        try {
            Assert-RenderPilotEqual -Actual $archive.Entries.Count -Expected 1 -Description "portable ZIP must contain exactly one entry"
            $entry = $archive.Entries[0]
            Assert-RenderPilotEqual -Actual $entry.FullName -Expected "RenderPilot/renderpilot-desktop.exe" `
                -Description "portable ZIP must contain the canonical payload entry"
            $entryBytes = [IO.MemoryStream]::new()
            try {
                $entryStream = $entry.Open()
                try {
                    $entryStream.CopyTo($entryBytes)
                }
                finally {
                    $entryStream.Dispose()
                }
                $entryContent = $entryBytes.ToArray()
            }
            finally {
                $entryBytes.Dispose()
            }
            Assert-RenderPilotBytesEqual -Actual $entryContent -Expected $sourceBytes `
                -Description "portable ZIP entry bytes must equal the raw executable bytes"
            Assert-RenderPilotEqual -Actual ([Convert]::ToHexString([Security.Cryptography.SHA256]::HashData($entryContent))) `
                -Expected (Get-RenderPilotSha256 -Path $source) `
                -Description "portable ZIP entry SHA-256 must equal the raw executable SHA-256"
        }
        finally {
            $archive.Dispose()
        }
    }
    finally {
        $zipStream.Dispose()
    }

    Write-Output "Portable artifact helper tests passed."
}
finally {
    foreach ($file in $knownFiles) {
        if (Test-Path -LiteralPath $file -PathType Leaf) {
            Remove-Item -LiteralPath $file -Force -ErrorAction Stop
        }
    }
    foreach ($directory in $knownDirectories | Sort-Object -Descending) {
        if (Test-Path -LiteralPath $directory -PathType Container) {
            Remove-Item -LiteralPath $directory -Force -ErrorAction Stop
        }
    }
    if ($null -ne $ownedRoot -and (Test-Path -LiteralPath $ownedRoot -PathType Container)) {
        $resolvedCleanupRoot = (Resolve-Path -LiteralPath $ownedRoot).Path.TrimEnd(
            [IO.Path]::DirectorySeparatorChar,
            [IO.Path]::AltDirectorySeparatorChar
        )
        Assert-RenderPilotEqual -Actual $resolvedCleanupRoot -Expected $resolvedRoot `
            -Description "cleanup must target only the resolved test-owned root"
        Remove-Item -LiteralPath $resolvedCleanupRoot -Force -ErrorAction Stop
    }
}
