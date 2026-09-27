[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

if ($PSVersionTable.PSVersion -lt [version]"7.3" -or -not $IsWindows) {
    throw "Running release tests requires PowerShell 7.3+ on Windows."
}

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

Write-Host "Running prepare-release-assets tests..."

# 1. Test Resolve-RenderPilotRequiredFile
$tempFile = [System.IO.Path]::GetTempFileName()
try {
    $resolved = Resolve-RenderPilotRequiredFile "TempFile" $tempFile
    Assert-RenderPilotTrue -Condition (Test-Path -LiteralPath $resolved -PathType Leaf) `
        -Description "Resolve-RenderPilotRequiredFile must return existing leaf path"

    Assert-RenderPilotThrows -Action {
        Resolve-RenderPilotRequiredFile "Missing" ""
    } -Description "Resolve-RenderPilotRequiredFile must throw on empty string"

    Assert-RenderPilotThrows -Action {
        Resolve-RenderPilotRequiredFile "Missing" "C:\nonexistent\path\file.txt"
    } -Description "Resolve-RenderPilotRequiredFile must throw on non-existent path"

    $tempDir = [System.IO.Path]::GetTempPath()
    Assert-RenderPilotThrows -Action {
        Resolve-RenderPilotRequiredFile "NotAFile" $tempDir
    } -Description "Resolve-RenderPilotRequiredFile must throw when given a directory"
}
finally {
    if (Test-Path -LiteralPath $tempFile) {
        Remove-Item -LiteralPath $tempFile -Force -ErrorAction SilentlyContinue
    }
}

# 2. Test Resolve-RenderPilotRequiredDirectory
$tempDir = [System.IO.Path]::GetTempPath()
$resolvedDir = Resolve-RenderPilotRequiredDirectory "TempDir" $tempDir
Assert-RenderPilotTrue -Condition (Test-Path -LiteralPath $resolvedDir -PathType Container) `
    -Description "Resolve-RenderPilotRequiredDirectory must return existing container path"

Assert-RenderPilotThrows -Action {
    Resolve-RenderPilotRequiredDirectory "Missing" ""
} -Description "Resolve-RenderPilotRequiredDirectory must throw on empty string"

Assert-RenderPilotThrows -Action {
    Resolve-RenderPilotRequiredDirectory "Missing" "C:\nonexistent\directory\"
} -Description "Resolve-RenderPilotRequiredDirectory must throw on non-existent container"

$tempFile2 = [System.IO.Path]::GetTempFileName()
try {
    Assert-RenderPilotThrows -Action {
        Resolve-RenderPilotRequiredDirectory "NotADir" $tempFile2
    } -Description "Resolve-RenderPilotRequiredDirectory must throw when given a leaf file"
}
finally {
    if (Test-Path -LiteralPath $tempFile2) {
        Remove-Item -LiteralPath $tempFile2 -Force -ErrorAction SilentlyContinue
    }
}

# 3. Test Invoke-RenderPilotTimedStep
$script:timedRan = $false
Invoke-RenderPilotTimedStep "Test operation" {
    $script:timedRan = $true
}
Assert-RenderPilotTrue -Condition $script:timedRan -Description "Invoke-RenderPilotTimedStep must execute its scriptblock"

# 4. Test renderpilot-updater-verifier
$repository = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
Push-Location $repository
try {
    $verifierBinary = Build-RenderPilotUpdaterVerifier
}
finally {
    Pop-Location
}

$dummyArtifact = [System.IO.Path]::GetTempFileName()
$dummySig = [System.IO.Path]::GetTempFileName()
try {
    & {
        $PSNativeCommandUseErrorActionPreference = $false

        # Rejection of invalid signature
        [System.IO.File]::WriteAllText($dummyArtifact, "test-content")
        [System.IO.File]::WriteAllText($dummySig, "dW50cnVzdGVkIGNvbW1lbnQ6IGJvZ3VzCg==")
        & $verifierBinary $dummyArtifact $dummySig
        Assert-RenderPilotEqual -Actual $LASTEXITCODE -Expected 1 -Description "Verifier must exit with code 1 on invalid signature"

        # Rejection of missing arguments
        & $verifierBinary
        Assert-RenderPilotEqual -Actual $LASTEXITCODE -Expected 1 -Description "Verifier must exit with code 1 when invoked with missing arguments"
    }
}
finally {
    Remove-Item -LiteralPath $dummyArtifact -Force -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $dummySig -Force -ErrorAction SilentlyContinue
}

# 5. Test prepare-release-assets.ps1 parameter interface contract
$scriptPath = (Resolve-Path (Join-Path $PSScriptRoot "prepare-release-assets.ps1")).Path
$scriptCommand = Get-Command $scriptPath
$expectedParams = @(
    "Version",
    "Tag",
    "Repository",
    "Commit",
    "GitHubSha",
    "RunId",
    "PublishedAt",
    "ChangelogPath",
    "TauriArtifactPathsJson",
    "RepositoryRoot",
    "ArtifactDirectory",
    "PortableRaw",
    "PortableRawSignature",
    "PortableRpu",
    "PortableRpuSignature",
    "PortableZip"
)
foreach ($paramName in $expectedParams) {
    Assert-RenderPilotTrue -Condition ($scriptCommand.Parameters.ContainsKey($paramName)) `
        -Description "prepare-release-assets.ps1 must declare parameter '$paramName'"
}

Write-Host "All prepare-release-assets tests passed successfully."

# Negative verifier tests above intentionally leave $LASTEXITCODE = 1.
# Reset ambient exit code so the GitHub Actions runner does not fail the step.
$global:LASTEXITCODE = 0
