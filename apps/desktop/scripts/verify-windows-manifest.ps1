[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Path,
    [Parameter(Mandatory)] [ValidateSet("asInvoker", "requireAdministrator")] [string] $ExpectedExecutionLevel,
    [string] $MtExe
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
. (Join-Path $PSScriptRoot "windows-manifest-common.ps1")

if (-not $IsWindows) {
    throw "Windows manifest resource verification requires Windows."
}
if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) {
    throw "Windows executable was not found: $Path"
}

function Resolve-RenderPilotMtExe {
    param([string] $RequestedPath)

    if (-not [string]::IsNullOrWhiteSpace($RequestedPath)) {
        return (Resolve-Path -LiteralPath $RequestedPath -ErrorAction Stop).Path
    }

    $command = Get-Command mt.exe -ErrorAction SilentlyContinue
    if ($null -ne $command) {
        return $command.Source
    }

    $kitRoots = @(
        (Join-Path ${env:ProgramFiles(x86)} "Windows Kits\\10\\bin"),
        (Join-Path $env:ProgramFiles "Windows Kits\\10\\bin")
    ) | Where-Object { -not [string]::IsNullOrWhiteSpace($_) -and (Test-Path -LiteralPath $_ -PathType Container) }
    $candidate = @(
        Get-ChildItem -LiteralPath $kitRoots -Filter mt.exe -File -Recurse -ErrorAction SilentlyContinue |
            Where-Object { $_.Directory.Name -ceq "x64" } |
            Sort-Object -Property FullName -Descending |
            Select-Object -First 1
    )
    if ($candidate.Count -eq 1) {
        return $candidate[0].FullName
    }

    throw "mt.exe was not found. Install the Windows SDK or pass -MtExe explicitly."
}

$executable = (Resolve-Path -LiteralPath $Path).Path
$resourceManifest = Join-Path ([IO.Path]::GetTempPath()) ("renderpilot-manifest-{0}.xml" -f [Guid]::NewGuid().ToString("N"))
try {
    $mt = Resolve-RenderPilotMtExe -RequestedPath $MtExe
    # Native mt.exe always sets this value. Initializing it also keeps a
    # deterministic script-double invocation from reading an unset variable.
    $LASTEXITCODE = 0
    & $mt -nologo "-inputresource:$executable;#1" "-out:$resourceManifest"
    if ($LASTEXITCODE -ne 0) {
        throw "mt.exe could not extract resource #1 from $executable (exit $LASTEXITCODE)."
    }
    Test-RenderPilotWindowsManifestXml `
        -ManifestPath $resourceManifest `
        -ExpectedExecutionLevel $ExpectedExecutionLevel
}
finally {
    if (Test-Path -LiteralPath $resourceManifest -PathType Leaf) {
        Remove-Item -LiteralPath $resourceManifest -Force -ErrorAction Stop
    }
}
