Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-RenderPilotWithWindowsManifest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [ValidateSet("production", "release-tooling")] [string] $Selector,
        [Parameter(Mandatory)] [scriptblock] $Command
    )

    $environmentPath = "Env:RENDERPILOT_WINDOWS_MANIFEST"
    $hadPreviousValue = Test-Path -LiteralPath $environmentPath
    $previousValue = if ($hadPreviousValue) { $env:RENDERPILOT_WINDOWS_MANIFEST } else { $null }

    try {
        Set-Item -LiteralPath $environmentPath -Value $Selector
        & $Command
    }
    finally {
        if ($hadPreviousValue) {
            Set-Item -LiteralPath $environmentPath -Value $previousValue
        }
        else {
            Remove-Item -LiteralPath $environmentPath -ErrorAction SilentlyContinue
        }
    }
}

function Test-RenderPilotWindowsManifestXml {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $ManifestPath,
        [Parameter(Mandatory)] [ValidateSet("asInvoker", "requireAdministrator")] [string] $ExpectedExecutionLevel
    )

    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) {
        throw "Windows manifest was not found: $ManifestPath"
    }

    try {
        [xml] $manifest = Get-Content -LiteralPath $ManifestPath -Raw
    }
    catch {
        throw "Windows manifest is not valid XML: $ManifestPath ($($_.Exception.Message))"
    }

    $executionLevels = @($manifest.SelectNodes("//*[local-name()='requestedExecutionLevel']"))
    if ($executionLevels.Count -ne 1) {
        throw "Windows manifest must contain exactly one requestedExecutionLevel; found $($executionLevels.Count): $ManifestPath"
    }
    $executionLevel = $executionLevels[0]
    if ($executionLevel.GetAttribute("level") -cne $ExpectedExecutionLevel) {
        throw "Windows manifest requestedExecutionLevel must be ${ExpectedExecutionLevel}: $ManifestPath"
    }
    if ($executionLevel.GetAttribute("uiAccess") -cne "false") {
        throw "Windows manifest requestedExecutionLevel must set uiAccess=false: $ManifestPath"
    }

    $commonControls = @($manifest.SelectNodes("//*[local-name()='assemblyIdentity' and @name='Microsoft.Windows.Common-Controls']"))
    if ($commonControls.Count -ne 1) {
        throw "Windows manifest must contain exactly one Common Controls v6 dependency; found $($commonControls.Count): $ManifestPath"
    }
    $commonControls = $commonControls[0]
    $expectedAttributes = [ordered]@{
        type = "win32"
        version = "6.0.0.0"
        processorArchitecture = "*"
        publicKeyToken = "6595b64144ccf1df"
        language = "*"
    }
    foreach ($attribute in $expectedAttributes.GetEnumerator()) {
        if ($commonControls.GetAttribute($attribute.Key) -cne $attribute.Value) {
            throw "Windows manifest Common Controls dependency must set $($attribute.Key)=$($attribute.Value): $ManifestPath"
        }
    }
}
