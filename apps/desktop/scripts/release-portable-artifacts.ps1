[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Version,
    [string] $RepositoryRoot,
    [string] $ArtifactDirectory,
    [string] $GitHubEnvironmentPath = $env:GITHUB_ENV
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "release-helpers.ps1")
. (Join-Path $PSScriptRoot "windows-manifest-common.ps1")

if ($PSVersionTable.PSVersion.Major -lt 7) {
    throw "Portable release artifacts require PowerShell 7 (pwsh), not Windows PowerShell 5.1."
}
if (-not $IsWindows) {
    throw "Portable release artifacts require Windows."
}

if ([string]::IsNullOrWhiteSpace($RepositoryRoot)) {
    $RepositoryRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path
}
$repository = (Resolve-Path -LiteralPath $RepositoryRoot).Path
$desktop = Join-Path $repository "apps\desktop"
if (-not (Test-Path -LiteralPath (Join-Path $desktop "src-tauri\Cargo.toml") -PathType Leaf)) {
    throw "RenderPilot desktop manifest was not found under $repository."
}
$runtimeRelease = Get-RenderPilotPortableRuntimeReleaseContract -RepositoryRoot $repository

$temporaryRoot = if ([string]::IsNullOrWhiteSpace($env:RUNNER_TEMP)) {
    [IO.Path]::GetTempPath()
}
else {
    $env:RUNNER_TEMP
}
if ([string]::IsNullOrWhiteSpace($ArtifactDirectory)) {
    $ArtifactDirectory = Join-Path $temporaryRoot "renderpilot-portable-release"
}
if (Test-Path -LiteralPath $ArtifactDirectory) {
    throw "Portable release artifact directory already exists: $ArtifactDirectory"
}
$artifacts = (New-Item -ItemType Directory -Path $ArtifactDirectory).FullName

function Set-GitHubEnvironmentValue {
    param(
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $Value
    )

    if (-not [string]::IsNullOrWhiteSpace($GitHubEnvironmentPath)) {
        "$Name=$Value" | Out-File -FilePath $GitHubEnvironmentPath -Encoding utf8 -Append
    }
}

Push-Location $desktop
try {
    Invoke-RenderPilotWithWindowsManifest -Selector production -Command {
        Invoke-RenderPilotCheckedCommand -Description "Building portable executable" -Command {
            pnpm tauri build --features portable --no-bundle
        }
    }
}
finally {
    Pop-Location
}

$portableSource = Join-Path $repository "target\release\renderpilot-desktop.exe"
if (-not (Test-Path -LiteralPath $portableSource -PathType Leaf)) {
    throw "Tauri did not produce the portable App image $portableSource."
}
Test-RenderPilotPeVersion -Path $portableSource -ExpectedVersion $Version -Property FileVersionRaw |
    Out-Null
& (Join-Path $PSScriptRoot "verify-windows-manifest.ps1") `
    -Path $portableSource `
    -ExpectedExecutionLevel requireAdministrator

$portableRpu = Join-Path $artifacts "RenderPilot_${Version}_x64-portable.rpu"
$portableRpuSignature = "$portableRpu.sig"
$portableExecutable = Join-Path $artifacts "RenderPilot_${Version}_x64-portable.exe"
$portableRawSignature = "$portableExecutable.sig"
$portableZip = Join-Path $artifacts "RenderPilot_${Version}_x64-portable.zip"
$appHash = Get-RenderPilotSha256 -Path $portableSource
$rpuManifest = New-RenderPilotPortableRpuManifest `
    -Version $Version `
    -RuntimeRelease $runtimeRelease `
    -AppSha256 $appHash `
    -AppLength ([IO.FileInfo]::new($portableSource).Length) |
    ConvertTo-Json -Compress
New-RenderPilotPortableRpu -App $portableSource -ManifestJson $rpuManifest -Destination $portableRpu

Push-Location $desktop
try {
    Invoke-RenderPilotCheckedCommand -Description "Signing public portable RPU" -Command {
        pnpm tauri signer sign $portableRpu
    }
}
finally {
    Pop-Location
}

if (-not (Test-Path -LiteralPath $portableRpuSignature -PathType Leaf)) {
    throw "Tauri signer did not create the portable RPU signature $portableRpuSignature."
}

# Canonicalize the public signature once. The SFX footer embeds these exact
# bytes, so an optional signer newline cannot create public/embedded drift.
$rpuSignatureText = (Get-Content -LiteralPath $portableRpuSignature -Raw).Trim()
[IO.File]::WriteAllText($portableRpuSignature, $rpuSignatureText, [Text.UTF8Encoding]::new($false))

Push-Location $repository
try {
    Invoke-RenderPilotWithWindowsManifest -Selector production -Command {
        Invoke-RenderPilotCheckedCommand -Description "Building stable portable supervisor" -Command {
            cargo build --locked --package renderpilot-desktop --bin portable-supervisor --features portable --release
        }
    }
    $supervisor = Join-Path $repository "target\release\portable-supervisor.exe"
    if (-not (Test-Path -LiteralPath $supervisor -PathType Leaf)) {
        throw "Cargo did not produce the stable portable supervisor $supervisor."
    }
    & (Join-Path $PSScriptRoot "verify-windows-manifest.ps1") `
        -Path $supervisor `
        -ExpectedExecutionLevel requireAdministrator
    Invoke-RenderPilotCheckedCommand -Description "Embedding exact public RPU in raw supervisor" -Command {
        node (Join-Path $repository "apps\desktop\scripts\portable-rpu.mjs") assemble `
            --supervisor $supervisor --rpu $portableRpu --signature $portableRpuSignature `
            --expected-version $Version --output $portableExecutable
    }
}
finally {
    Pop-Location
}

Test-RenderPilotPeVersion -Path $portableExecutable -ExpectedVersion $Version -Property FileVersionRaw |
    Out-Null

Push-Location $desktop
try {
    Invoke-RenderPilotCheckedCommand -Description "Signing raw portable supervisor" -Command {
        pnpm tauri signer sign $portableExecutable
    }
}
finally {
    Pop-Location
}
if (-not (Test-Path -LiteralPath $portableRawSignature -PathType Leaf)) {
    throw "Tauri signer did not create the raw portable supervisor signature $portableRawSignature."
}
$rawSignatureText = (Get-Content -LiteralPath $portableRawSignature -Raw).Trim()
[IO.File]::WriteAllText($portableRawSignature, $rawSignatureText, [Text.UTF8Encoding]::new($false))

& (Join-Path $PSScriptRoot "verify-windows-manifest.ps1") `
    -Path $portableExecutable `
    -ExpectedExecutionLevel requireAdministrator

$zipStagingRoot = New-RenderPilotUniqueStagingRoot `
    -Parent $temporaryRoot `
    -Prefix "renderpilot-portable-zip-"
$zipStagingDirectory = Join-Path $zipStagingRoot "RenderPilot"
New-Item -ItemType Directory -Path $zipStagingDirectory | Out-Null
$zipStagedExecutable = Join-Path $zipStagingDirectory "renderpilot-desktop.exe"
Copy-RenderPilotFileCreateNew -Source $portableExecutable -Destination $zipStagedExecutable
New-RenderPilotPortableZip `
    -Source $zipStagedExecutable `
    -Destination $portableZip `
    -EntryName "RenderPilot/renderpilot-desktop.exe"

Push-Location $repository
try {
    Invoke-RenderPilotCheckedCommand -Description "Validating raw/RPU/signature/ZIP identity" -Command {
        node (Join-Path $repository "apps\desktop\scripts\portable-rpu.mjs") validate `
            --raw $portableExecutable --rpu $portableRpu --signature $portableRpuSignature `
            --zip $portableZip --zip-entry "RenderPilot/renderpilot-desktop.exe" `
            --expected-version $Version
    }
}
finally {
    Pop-Location
}

Set-GitHubEnvironmentValue -Name "RENDERPILOT_PORTABLE_DIR" -Value $artifacts
Set-GitHubEnvironmentValue -Name "RENDERPILOT_PORTABLE_RAW" -Value $portableExecutable
Set-GitHubEnvironmentValue -Name "RENDERPILOT_PORTABLE_RAW_SIG" -Value $portableRawSignature
Set-GitHubEnvironmentValue -Name "RENDERPILOT_PORTABLE_RPU" -Value $portableRpu
Set-GitHubEnvironmentValue -Name "RENDERPILOT_PORTABLE_RPU_SIG" -Value $portableRpuSignature
Set-GitHubEnvironmentValue -Name "RENDERPILOT_PORTABLE_ZIP" -Value $portableZip

[pscustomobject]@{
    artifactDirectory = $artifacts
    raw = $portableExecutable
    rawSignature = $portableRawSignature
    rpu = $portableRpu
    rpuSignature = $portableRpuSignature
    zip = $portableZip
} | ConvertTo-Json -Compress
