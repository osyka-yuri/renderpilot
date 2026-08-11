[CmdletBinding()]
param()

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows-manifest-common.ps1")

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
    Assert-RenderPilotTrue -Condition $threw -Description $Description
}

$developmentManifest = Join-Path $PSScriptRoot "..\\src-tauri\\build-support\\windows-manifests\\development.manifest.xml"
$productionManifest = Join-Path $PSScriptRoot "..\\src-tauri\\build-support\\windows-manifests\\production.manifest.xml"
$environmentPath = "Env:RENDERPILOT_WINDOWS_MANIFEST"
$hadPreviousValue = Test-Path -LiteralPath $environmentPath
$previousValue = if ($hadPreviousValue) { $env:RENDERPILOT_WINDOWS_MANIFEST } else { $null }
$invalidManifest = Join-Path ([IO.Path]::GetTempPath()) ("renderpilot-invalid-manifest-{0}.xml" -f [Guid]::NewGuid().ToString("N"))
$fakeMtRoot = Join-Path ([IO.Path]::GetTempPath()) ("renderpilot-manifest-mt-test-{0}" -f [Guid]::NewGuid().ToString("N"))

try {
    Test-RenderPilotWindowsManifestXml -ManifestPath $developmentManifest -ExpectedExecutionLevel asInvoker
    Test-RenderPilotWindowsManifestXml -ManifestPath $productionManifest -ExpectedExecutionLevel requireAdministrator

    [IO.File]::WriteAllText(
        $invalidManifest,
        '<assembly><requestedExecutionLevel level="asInvoker" uiAccess="true" /></assembly>',
        [Text.UTF8Encoding]::new($false)
    )
    Assert-RenderPilotThrows -Description "manifest verifier must reject uiAccess=true and a missing Common Controls dependency" -Action {
        Test-RenderPilotWindowsManifestXml -ManifestPath $invalidManifest -ExpectedExecutionLevel asInvoker
    }

    Remove-Item -LiteralPath $environmentPath -ErrorAction SilentlyContinue
    $observedSelector = Invoke-RenderPilotWithWindowsManifest -Selector production -Command {
        $env:RENDERPILOT_WINDOWS_MANIFEST
    }
    Assert-RenderPilotTrue -Condition ($observedSelector -ceq "production") `
        -Description "production selector must be visible only inside its scoped command"
    Assert-RenderPilotTrue -Condition (-not (Test-Path -LiteralPath $environmentPath)) `
        -Description "scoped selector must remove an originally absent environment value"

    Set-Item -LiteralPath $environmentPath -Value "release-tooling"
    Assert-RenderPilotThrows -Description "scoped selector must restore its previous value when the command fails" -Action {
        Invoke-RenderPilotWithWindowsManifest -Selector production -Command {
            throw "expected test failure"
        }
    }
    Assert-RenderPilotTrue -Condition ($env:RENDERPILOT_WINDOWS_MANIFEST -ceq "release-tooling") `
        -Description "scoped selector must restore its prior environment value after failure"

    $fakeMtRoot = (New-Item -ItemType Directory -Path $fakeMtRoot -ErrorAction Stop).FullName
    $fakeExecutable = Join-Path $fakeMtRoot "renderpilot-desktop.exe"
    $fakeMt = Join-Path $fakeMtRoot "mt.exe.ps1"
    $fakeManifest = Join-Path $fakeMtRoot "resource-1.manifest.xml"
    [IO.File]::WriteAllBytes($fakeExecutable, [byte[]](0))
    Copy-Item -LiteralPath $productionManifest -Destination $fakeManifest -ErrorAction Stop
    [IO.File]::WriteAllText(
        $fakeMt,
        @'
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$inputResourceArgument = @($args | Where-Object { $_ -like "-inputresource:*" } | Select-Object -First 1)
$outArgument = @($args | Where-Object { $_ -like "-out:*" } | Select-Object -First 1)
if ($inputResourceArgument.Count -ne 1 -or $outArgument.Count -ne 1) {
    throw "Manifest verifier must pass mt.exe inputresource and out arguments."
}
$inputResource = $inputResourceArgument[0].Substring("-inputresource:".Length)
if (-not $inputResource.EndsWith(";#1", [StringComparison]::Ordinal)) {
    throw "Manifest verifier must request resource #1."
}
$outPath = $outArgument[0].Substring("-out:".Length)
Copy-Item -LiteralPath (Join-Path $PSScriptRoot "resource-1.manifest.xml") -Destination $outPath -ErrorAction Stop
'@,
        [Text.UTF8Encoding]::new($false)
    )

    & (Join-Path $PSScriptRoot "verify-windows-manifest.ps1") `
        -Path $fakeExecutable `
        -ExpectedExecutionLevel requireAdministrator `
        -MtExe $fakeMt
    Assert-RenderPilotThrows -Description "mt.exe verifier must reject an extracted execution level that differs from the expected App contract" -Action {
        & (Join-Path $PSScriptRoot "verify-windows-manifest.ps1") `
            -Path $fakeExecutable `
            -ExpectedExecutionLevel asInvoker `
            -MtExe $fakeMt
    }

    Write-Output "Windows manifest source and scoped selector tests passed."
}
finally {
    if (Test-Path -LiteralPath $invalidManifest -PathType Leaf) {
        Remove-Item -LiteralPath $invalidManifest -Force -ErrorAction Stop
    }
    if (Test-Path -LiteralPath $fakeMtRoot -PathType Container) {
        Remove-Item -LiteralPath $fakeMtRoot -Force -Recurse -ErrorAction Stop
    }
    if ($hadPreviousValue) {
        Set-Item -LiteralPath $environmentPath -Value $previousValue
    }
    else {
        Remove-Item -LiteralPath $environmentPath -ErrorAction SilentlyContinue
    }
}
