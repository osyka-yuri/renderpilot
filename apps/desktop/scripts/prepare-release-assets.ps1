[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Version,
    [Parameter(Mandatory)] [string] $Tag,
    [Parameter(Mandatory)] [string] $Repository,
    [Parameter(Mandatory)] [string] $Commit,
    [Parameter(Mandatory)] [string] $GitHubSha,
    [Parameter(Mandatory)] [string] $RunId,
    [Parameter(Mandatory)] [string] $PublishedAt,
    [Parameter(Mandatory)] [string] $ChangelogPath,
    [Parameter(Mandatory)] [string] $TauriArtifactPathsJson,
    [string] $RepositoryRoot,
    [string] $ArtifactDirectory = $env:RENDERPILOT_PORTABLE_DIR,
    [string] $PortableRaw = $env:RENDERPILOT_PORTABLE_RAW,
    [string] $PortableRawSignature = $env:RENDERPILOT_PORTABLE_RAW_SIG,
    [string] $PortableRpu = $env:RENDERPILOT_PORTABLE_RPU,
    [string] $PortableRpuSignature = $env:RENDERPILOT_PORTABLE_RPU_SIG,
    [string] $PortableZip = $env:RENDERPILOT_PORTABLE_ZIP
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "release-helpers.ps1")

if ($PSVersionTable.PSVersion.Major -lt 7 -or -not $IsWindows) {
    throw "Preparing RenderPilot release assets requires PowerShell 7 on Windows."
}
if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
}

$repositoryPath = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$artifactPath = (Resolve-Path -LiteralPath $ArtifactDirectory).Path
$releaseManifestScript = Join-Path $repositoryPath "apps\desktop\scripts\release-manifest.mjs"

foreach ($required in @($ChangelogPath, $PortableRaw, $PortableRawSignature, $PortableRpu, $PortableRpuSignature, $PortableZip, $releaseManifestScript)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Required release input was not found: $required"
    }
}

$selectionJson = & node $releaseManifestScript `
    "select-tauri-artifacts" `
    "--paths-json" $TauriArtifactPathsJson `
    "--version" $Version
if ($LASTEXITCODE -ne 0) {
    throw "Selecting current-run tauri-action artifacts failed with exit code $LASTEXITCODE."
}
$tauriArtifacts = $selectionJson | ConvertFrom-Json
$versionedInstaller = (Resolve-Path -LiteralPath $tauriArtifacts.installerPath).Path
$installerSignature = (Resolve-Path -LiteralPath $tauriArtifacts.installerSignaturePath).Path
foreach ($required in @($versionedInstaller, $installerSignature)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Current-run tauri-action artifact was not found: $required"
    }
}

$installerAlias = Join-Path $artifactPath "RenderPilot-setup.exe"
$outputManifest = Join-Path $artifactPath "latest.json"
$publicationSpecificationPath = Join-Path $artifactPath "publication-spec.json"
foreach ($output in @($installerAlias, $outputManifest, $publicationSpecificationPath)) {
    if (Test-Path -LiteralPath $output) {
        throw "Release preparation output path already exists: $output"
    }
}
Copy-RenderPilotFileCreateNew -Source $versionedInstaller -Destination $installerAlias
if ((Get-RenderPilotSha256 -Path $installerAlias) -ne (Get-RenderPilotSha256 -Path $versionedInstaller)) {
    throw "Stable installer alias does not match the versioned installer SHA-256."
}

Push-Location $repositoryPath
try {
    Invoke-RenderPilotCheckedCommand -Description "Generating deterministic updater metadata" -Command {
        node $releaseManifestScript transform `
            --output $outputManifest `
            --version $Version `
            --repository $Repository `
            --tag $Tag `
            --changelog $ChangelogPath `
            --published-at $PublishedAt `
            --installer $versionedInstaller `
            --installer-signature $installerSignature `
            --portable-raw $PortableRaw `
            --portable-raw-signature $PortableRawSignature `
            --portable-rpu $PortableRpu `
            --portable-rpu-signature $PortableRpuSignature `
            --portable-zip $PortableZip `
            --zip-entry "RenderPilot/renderpilot-desktop.exe"
    }
    Invoke-RenderPilotCheckedCommand -Description "Verifying NSIS installer signature" -Command {
        cargo run --quiet --package renderpilot-desktop --features updater-artifact-verify `
            --example verify_updater_signature -- $versionedInstaller $installerSignature
    }
    Invoke-RenderPilotCheckedCommand -Description "Verifying public portable RPU signature" -Command {
        cargo run --quiet --package renderpilot-desktop --features updater-artifact-verify `
            --example verify_updater_signature -- $PortableRpu $PortableRpuSignature
    }
    Invoke-RenderPilotCheckedCommand -Description "Verifying raw portable supervisor signature" -Command {
        cargo run --quiet --package renderpilot-desktop --features updater-artifact-verify `
            --example verify_updater_signature -- $PortableRaw $PortableRawSignature
    }

    $artifactPaths = @(
        $versionedInstaller,
        $installerSignature,
        $installerAlias,
        $PortableRaw,
        $PortableRawSignature,
        $PortableRpu,
        $PortableRpuSignature,
        $PortableZip,
        $outputManifest
    )
    $artifactNames = @($artifactPaths | ForEach-Object { [IO.Path]::GetFileName($_) })
    if (($artifactNames | Select-Object -Unique).Count -ne $artifactNames.Count) {
        throw "The release asset set contains duplicate filenames."
    }

    $publicationArgs = @(
        $releaseManifestScript,
        "publication-spec",
        "--changelog", $ChangelogPath,
        "--commit", $Commit,
        "--github-sha", $GitHubSha,
        "--published-at", $PublishedAt,
        "--repository", $Repository,
        "--run-id", $RunId,
        "--tag", $Tag,
        "--version", $Version
    )
    foreach ($artifact in $artifactPaths) {
        $publicationArgs += @("--artifact", $artifact)
    }

    $publicationJson = & node @publicationArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Constructing release publication specification failed with exit code $LASTEXITCODE."
    }
    $publicationJson | Set-Content -LiteralPath $publicationSpecificationPath -Encoding utf8 -NoNewline

    Write-Host "Successfully prepared, digest-locked, and verified all $($artifactPaths.Count) release distribution assets in $artifactPath."
}
finally {
    Pop-Location
}
