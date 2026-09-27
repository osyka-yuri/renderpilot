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

if ($PSVersionTable.PSVersion.Major -lt 7 -or -not $IsWindows) {
    throw "Preparing RenderPilot release assets requires PowerShell 7 on Windows."
}

. (Join-Path $PSScriptRoot "release-helpers.ps1")

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
}

$repositoryPath = Resolve-RenderPilotRequiredDirectory "RepositoryRoot" $RepositoryRoot
$artifactPath = Resolve-RenderPilotRequiredDirectory "ArtifactDirectory" $ArtifactDirectory
$releaseManifestScript = Resolve-RenderPilotRequiredFile "ReleaseManifestScript" (Join-Path $repositoryPath "apps\desktop\scripts\release-manifest.mjs")

$ChangelogPath = Resolve-RenderPilotRequiredFile "ChangelogPath" $ChangelogPath
$PortableRaw = Resolve-RenderPilotRequiredFile "PortableRaw" $PortableRaw
$PortableRawSignature = Resolve-RenderPilotRequiredFile "PortableRawSignature" $PortableRawSignature
$PortableRpu = Resolve-RenderPilotRequiredFile "PortableRpu" $PortableRpu
$PortableRpuSignature = Resolve-RenderPilotRequiredFile "PortableRpuSignature" $PortableRpuSignature
$PortableZip = Resolve-RenderPilotRequiredFile "PortableZip" $PortableZip

$selectionJson = & node $releaseManifestScript `
    "select-tauri-artifacts" `
    "--paths-json" $TauriArtifactPathsJson `
    "--version" $Version
if ($LASTEXITCODE -ne 0) {
    throw "Selecting current-run tauri-action artifacts failed with exit code $LASTEXITCODE."
}
$tauriArtifacts = $selectionJson | ConvertFrom-Json
if ($null -eq $tauriArtifacts.PSObject.Properties['installerPath'] -or [string]::IsNullOrWhiteSpace($tauriArtifacts.installerPath)) {
    throw "select-tauri-artifacts did not return an installerPath."
}
if ($null -eq $tauriArtifacts.PSObject.Properties['installerSignaturePath'] -or [string]::IsNullOrWhiteSpace($tauriArtifacts.installerSignaturePath)) {
    throw "select-tauri-artifacts did not return an installerSignaturePath."
}

$versionedInstaller = Resolve-RenderPilotRequiredFile "Tauri installer" $tauriArtifacts.installerPath
$installerSignature = Resolve-RenderPilotRequiredFile "Tauri installer signature" $tauriArtifacts.installerSignaturePath

$installerAlias = Join-Path $artifactPath "RenderPilot-setup.exe"
$outputManifest = Join-Path $artifactPath "latest.json"
$publicationSpecificationPath = Join-Path $artifactPath "publication-spec.json"
foreach ($output in @($installerAlias, $outputManifest, $publicationSpecificationPath)) {
    if (Test-Path -LiteralPath $output) {
        throw "Release preparation output path already exists: $output"
    }
}

try {
    Copy-RenderPilotFileCreateNew -Source $versionedInstaller -Destination $installerAlias
    if ((Get-RenderPilotSha256 -Path $installerAlias) -ne (Get-RenderPilotSha256 -Path $versionedInstaller)) {
        throw "Stable installer alias does not match the versioned installer SHA-256."
    }

    Push-Location $repositoryPath
    try {
        Invoke-RenderPilotTimedStep "Generating deterministic updater metadata" {
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
        }

        $verifierBinary = Invoke-RenderPilotTimedStep "Building updater artifact verifier" {
            Build-RenderPilotUpdaterVerifier
        }

        Invoke-RenderPilotTimedStep "Verifying NSIS installer signature" {
            Invoke-RenderPilotCheckedCommand -Description "Verifying NSIS installer signature" -Command {
                & $verifierBinary $versionedInstaller $installerSignature
            }
        }
        Invoke-RenderPilotTimedStep "Verifying public portable RPU signature" {
            Invoke-RenderPilotCheckedCommand -Description "Verifying public portable RPU signature" -Command {
                & $verifierBinary $PortableRpu $PortableRpuSignature
            }
        }
        Invoke-RenderPilotTimedStep "Verifying raw portable supervisor signature" {
            Invoke-RenderPilotCheckedCommand -Description "Verifying raw portable supervisor signature" -Command {
                & $verifierBinary $PortableRaw $PortableRawSignature
            }
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
        $seenNames = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
        foreach ($name in $artifactNames) {
            if (-not $seenNames.Add($name)) {
                throw "The release asset set contains duplicate filename: $name"
            }
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
        $null = $publicationJson | ConvertFrom-Json
        $publicationJson | Set-Content -LiteralPath $publicationSpecificationPath -Encoding utf8 -NoNewline

        Write-Host "Successfully prepared, digest-locked, and verified all $($artifactPaths.Count) release distribution assets in $artifactPath."
    }
    finally {
        Pop-Location
    }
}
catch {
    foreach ($output in @($installerAlias, $outputManifest, $publicationSpecificationPath)) {
        if (Test-Path -LiteralPath $output) {
            try {
                Remove-Item -LiteralPath $output -Force -ErrorAction Stop
            }
            catch {
                Write-Warning "Failed to clean up release preparation output '$output': $_"
            }
        }
    }
    throw
}
