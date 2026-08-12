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
Import-Module (Join-Path $PSScriptRoot "release-github-client.psm1") -Force

if ($PSVersionTable.PSVersion.Major -lt 7 -or -not $IsWindows) {
    throw "Publishing RenderPilot release assets requires PowerShell 7 on Windows."
}
if ([string]::IsNullOrWhiteSpace($env:GH_TOKEN)) {
    throw "Publishing RenderPilot release assets requires the authenticated GH_TOKEN environment variable."
}
$gitHubToken = $env:GH_TOKEN
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

function Invoke-GitHubAssetUpload {
    param(
        [Parameter(Mandatory)] [long] $ReleaseId,
        [Parameter(Mandatory)] [string] $Artifact
    )

    $name = [IO.Path]::GetFileName($Artifact)
    $encodedName = [uri]::EscapeDataString($name)
    return Invoke-RenderPilotGitHubJson `
        -Method POST `
        -Endpoint "repos/$Repository/releases/$ReleaseId/assets?name=$encodedName" `
        -Token $gitHubToken `
        -Upload `
        -InputFile $Artifact
}

function Get-GitHubReleaseByTag {
    param([Parameter(Mandatory)] [string] $ReleaseTag)

    $lookup = Get-RenderPilotGitHubReleaseByTag `
        -Repository $Repository `
        -Tag $ReleaseTag `
        -Token $gitHubToken
    if (-not $lookup.Found) {
        return $null
    }
    return $lookup.Release
}

function Get-GitHubReleaseById {
    param([Parameter(Mandatory)] [long] $ReleaseId)

    return Get-RenderPilotGitHubReleaseById `
        -Repository $Repository `
        -ReleaseId $ReleaseId `
        -Token $gitHubToken
}

function Assert-ReleaseState {
    param(
        [Parameter(Mandatory)] $Release,
        [Parameter(Mandatory)] [ValidateSet("staging", "final")] [string] $State,
        [Parameter(Mandatory)] [string] $SpecificationPath,
        [string[]] $ExpectedArtifacts = @()
    )

    $arguments = @(
        $releaseManifestScript,
        "assert-publication-state",
        "--state", $State,
        "--release", "-",
        "--spec", $SpecificationPath
    )
    foreach ($artifact in $ExpectedArtifacts) {
        $arguments += @("--artifact", $artifact)
    }
    $releaseJson = $Release | ConvertTo-Json -Depth 100 -Compress
    $releaseJson | & node @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "GitHub $State release does not match the current-run publication specification."
    }
}

function Assert-TagCommit {
    param(
        [Parameter(Mandatory)] [string] $ReleaseTag,
        [Parameter(Mandatory)] [string] $ExpectedCommit
    )

    Assert-RenderPilotGitHubPeeledTagCommit `
        -Repository $Repository `
        -Tag $ReleaseTag `
        -Token $gitHubToken `
        -ExpectedCommit $ExpectedCommit | Out-Null
}

function Test-FinalAlreadyPublished {
    param(
        [Parameter(Mandatory)] [string] $SpecificationPath,
        [Parameter(Mandatory)] [string[]] $ExpectedArtifacts
    )

    $final = Get-GitHubReleaseByTag -ReleaseTag $Tag
    if ($null -eq $final) {
        return $false
    }
    Assert-ReleaseState `
        -Release $final `
        -State "final" `
        -SpecificationPath $SpecificationPath `
        -ExpectedArtifacts $ExpectedArtifacts
    Assert-TagCommit -ReleaseTag $Tag -ExpectedCommit $initialTagCommit
    Write-Host "Final release $Tag is already published with the exact current-run asset set."
    return $true
}

function Get-ExactStagingRelease {
    param(
        [Parameter(Mandatory)] [long] $ReleaseId,
        [Parameter(Mandatory)] [string] $SpecificationPath,
        [Parameter(Mandatory)] [string[]] $ExpectedArtifacts
    )

    $refetched = Get-GitHubReleaseById -ReleaseId $ReleaseId
    $refetchedPath = Join-Path $artifactPath "refetched-release-$ReleaseId.json"
    $refetched | ConvertTo-Json -Depth 100 -Compress | Set-Content -LiteralPath $refetchedPath -Encoding utf8 -NoNewline
    $arguments = @(
        $releaseManifestScript,
        "classify-publication-state",
        "--release", $refetchedPath,
        "--spec", $SpecificationPath
    )
    foreach ($artifact in $ExpectedArtifacts) {
        $arguments += @("--artifact", $artifact)
    }
    $classificationJson = & node @arguments
    if ($LASTEXITCODE -ne 0) {
        throw "Refetched release ID $ReleaseId is neither the exact staging draft nor the exact final publication."
    }
    $classification = $classificationJson | ConvertFrom-Json
    if ($classification.state -eq "staging") {
        return $refetched
    }
    if ($classification.state -eq "final") {
        Assert-TagCommit -ReleaseTag $Tag -ExpectedCommit $initialTagCommit
        Write-Host "Release ID $ReleaseId was atomically published as the exact final release $Tag."
        return $null
    }
    throw "Refetched release ID $ReleaseId returned unknown publication state '$($classification.state)'."
}

function Get-OrCreateExactStagingRelease {
    param(
        [Parameter(Mandatory)] $Publication,
        [Parameter(Mandatory)] [string] $SpecificationPath,
        [Parameter(Mandatory)] [string[]] $ExpectedArtifacts
    )

    if (Test-FinalAlreadyPublished -SpecificationPath $SpecificationPath -ExpectedArtifacts $ExpectedArtifacts) {
        return $null
    }
    $staging = Get-GitHubReleaseByTag -ReleaseTag $Publication.staging.tag_name
    if ($null -ne $staging) {
        Assert-ReleaseState -Release $staging -State "staging" -SpecificationPath $SpecificationPath
        return $staging
    }
    if (Test-FinalAlreadyPublished -SpecificationPath $SpecificationPath -ExpectedArtifacts $ExpectedArtifacts) {
        return $null
    }

    $created = Invoke-RenderPilotGitHubJson `
        -Method POST `
        -Endpoint "repos/$Repository/releases" `
        -Token $gitHubToken `
        -Body $Publication.staging
    if ($created.Succeeded) {
        $staging = $created.Json
    }
    else {
        # A failed or ambiguous create may still have reached GitHub. Reconcile
        # it instead of issuing a second create request.
        $staging = Get-GitHubReleaseByTag -ReleaseTag $Publication.staging.tag_name
        if ($null -eq $staging) {
            if (Test-FinalAlreadyPublished -SpecificationPath $SpecificationPath -ExpectedArtifacts $ExpectedArtifacts) {
                return $null
            }
            throw "Creating private staging release failed and did not produce the exact current-run draft or final release: $($created.Error)"
        }
    }
    Assert-ReleaseState -Release $staging -State "staging" -SpecificationPath $SpecificationPath
    return $staging
}

function Publish-CreateOnlyReleaseAsset {
    param(
        [Parameter(Mandatory)] [long] $ReleaseId,
        [Parameter(Mandatory)] [string] $Artifact,
        [Parameter(Mandatory)] [string] $SpecificationPath,
        [Parameter(Mandatory)] [string[]] $ExpectedArtifacts
    )

    if (Test-FinalAlreadyPublished `
            -SpecificationPath $SpecificationPath `
            -ExpectedArtifacts $ExpectedArtifacts) {
        return $true
    }
    $staging = Get-ExactStagingRelease `
        -ReleaseId $ReleaseId `
        -SpecificationPath $SpecificationPath `
        -ExpectedArtifacts $ExpectedArtifacts
    if ($null -eq $staging) {
        return $true
    }
    $releaseJsonPath = Join-Path $artifactPath "staging-release-$ReleaseId.json"
    $staging | ConvertTo-Json -Depth 100 -Compress | Set-Content -LiteralPath $releaseJsonPath -Encoding utf8 -NoNewline
    $planJson = & node $releaseManifestScript "plan-upload" --release $releaseJsonPath --artifact $Artifact
    if ($LASTEXITCODE -ne 0) {
        throw "Planning create-only upload for $(Split-Path -Leaf $Artifact) failed with exit code $LASTEXITCODE."
    }
    $plan = $planJson | ConvertFrom-Json
    if ($plan.action -eq "skip") {
        Write-Host "Release asset $(Split-Path -Leaf $Artifact) already has identical bytes; skipping upload."
        return $false
    }
    if ($plan.action -ne "upload") {
        throw "Release publication planner returned unknown action '$($plan.action)'."
    }

    $uploaded = Invoke-GitHubAssetUpload -ReleaseId $ReleaseId -Artifact $Artifact
    $after = Get-ExactStagingRelease `
        -ReleaseId $ReleaseId `
        -SpecificationPath $SpecificationPath `
        -ExpectedArtifacts $ExpectedArtifacts
    if ($null -eq $after) {
        return $true
    }
    $after | ConvertTo-Json -Depth 100 -Compress | Set-Content -LiteralPath $releaseJsonPath -Encoding utf8 -NoNewline
    $afterPlanJson = & node $releaseManifestScript "plan-upload" --release $releaseJsonPath --artifact $Artifact
    if ($LASTEXITCODE -ne 0) {
        throw "Rechecking create-only upload for $(Split-Path -Leaf $Artifact) failed with exit code $LASTEXITCODE."
    }
    $afterPlan = $afterPlanJson | ConvertFrom-Json
    if ($afterPlan.action -eq "skip") {
        return $false
    }
    if (-not $uploaded.Succeeded) {
        throw "Uploading release asset $(Split-Path -Leaf $Artifact) failed and refetch did not prove identical content: $($uploaded.Error)"
    }
    throw "Create-only upload for $(Split-Path -Leaf $Artifact) did not result in byte-identical release content."
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
        throw "Release publication output path already exists: $output"
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

    $publicationJson = & node $releaseManifestScript publication-spec `
        --changelog $ChangelogPath `
        --commit $Commit `
        --github-sha $GitHubSha `
        --published-at $PublishedAt `
        --repository $Repository `
        --run-id $RunId `
        --tag $Tag `
        --version $Version
    if ($LASTEXITCODE -ne 0) {
        throw "Constructing release publication specification failed with exit code $LASTEXITCODE."
    }
    $publicationJson | Set-Content -LiteralPath $publicationSpecificationPath -Encoding utf8 -NoNewline
    $publication = $publicationJson | ConvertFrom-Json
    $initialTagCommit = Assert-RenderPilotGitHubPeeledTagCommit `
        -Repository $Repository `
        -Tag $Tag `
        -Token $gitHubToken `
        -ExpectedCommit $Commit

    $staging = Get-OrCreateExactStagingRelease `
        -Publication $publication `
        -SpecificationPath $publicationSpecificationPath `
        -ExpectedArtifacts $artifactPaths
    if ($null -eq $staging) {
        return
    }
    $releaseId = [long] $staging.id

    foreach ($artifact in $artifactPaths) {
        if (Test-FinalAlreadyPublished `
                -SpecificationPath $publicationSpecificationPath `
                -ExpectedArtifacts $artifactPaths) {
            return
        }
        if (Publish-CreateOnlyReleaseAsset `
            -ReleaseId $releaseId `
            -Artifact $artifact `
            -SpecificationPath $publicationSpecificationPath `
            -ExpectedArtifacts $artifactPaths) {
            return
        }
    }

    if (Test-FinalAlreadyPublished `
            -SpecificationPath $publicationSpecificationPath `
            -ExpectedArtifacts $artifactPaths) {
        return
    }
    $ready = Get-ExactStagingRelease `
        -ReleaseId $releaseId `
        -SpecificationPath $publicationSpecificationPath `
        -ExpectedArtifacts $artifactPaths
    if ($null -eq $ready) {
        return
    }
    $readyAssetsPath = Join-Path $artifactPath "staging-assets-$releaseId.json"
    $ready.assets | ConvertTo-Json -Depth 100 -Compress | Set-Content -LiteralPath $readyAssetsPath -Encoding utf8 -NoNewline
    Invoke-RenderPilotCheckedCommand -Description "Verifying exact complete staging asset set" -Command {
        $arguments = @($releaseManifestScript, "verify-upload", "--assets", $readyAssetsPath, "--exact")
        foreach ($artifact in $artifactPaths) {
            $arguments += @("--artifact", $artifact)
        }
        & node @arguments
    }

    if (Test-FinalAlreadyPublished `
            -SpecificationPath $publicationSpecificationPath `
            -ExpectedArtifacts $artifactPaths) {
        return
    }
    # This is the sole irreversible release transition. Re-read the remote
    # peeled tag immediately before it, so a moved tag can never be published.
    Assert-TagCommit -ReleaseTag $Tag -ExpectedCommit $initialTagCommit
    $published = Invoke-RenderPilotGitHubJson `
        -Method PATCH `
        -Endpoint "repos/$Repository/releases/$releaseId" `
        -Token $gitHubToken `
        -Body $publication.final_request
    $final = Get-GitHubReleaseByTag -ReleaseTag $Tag
    if ($null -ne $final) {
        Assert-ReleaseState `
            -Release $final `
            -State "final" `
            -SpecificationPath $publicationSpecificationPath `
            -ExpectedArtifacts $artifactPaths
        Assert-TagCommit -ReleaseTag $Tag -ExpectedCommit $initialTagCommit
        Write-Host "Published verified RenderPilot release $Tag from staging release ID $releaseId."
        return
    }
    $finalizedById = Get-ExactStagingRelease `
        -ReleaseId $releaseId `
        -SpecificationPath $publicationSpecificationPath `
        -ExpectedArtifacts $artifactPaths
    if ($null -eq $finalizedById) {
        return
    }
    if (-not $published.Succeeded) {
        throw "Final publication PATCH failed and did not produce the exact final release: $($published.Error)"
    }
    throw "Final publication PATCH returned without the expected published release tag $Tag."
}
finally {
    Pop-Location
}
