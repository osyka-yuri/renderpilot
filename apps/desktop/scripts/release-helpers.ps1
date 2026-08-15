Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Invoke-RenderPilotCheckedCommand {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [scriptblock] $Command,
        [Parameter(Mandatory)] [string] $Description
    )

    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "$Description failed with exit code $LASTEXITCODE."
    }
}

function Get-RenderPilotSha256 {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Path)

    (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash
}

function Get-RenderPilotPortableRuntimeReleaseContract {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $RepositoryRoot)

    $path = Join-Path $RepositoryRoot "data\contracts\portable-runtime-release.json"
    Get-RenderPilotPortableRuntimeReleaseContractFromJson -Json (
        Get-Content -LiteralPath $path -Raw -ErrorAction Stop
    )
}

function Get-RenderPilotPortableRuntimeReleaseContractFromJson {
    [CmdletBinding()]
    param([Parameter(Mandatory)] [string] $Json)

    $document = [System.Text.Json.JsonDocument]::Parse($Json)
    try {
        $root = $document.RootElement
        if ($root.ValueKind -ne [System.Text.Json.JsonValueKind]::Object) {
            throw "Portable runtime release contract must be a JSON object."
        }
        $contract = [System.Collections.Generic.Dictionary[string, System.Text.Json.JsonElement]]::new(
            [System.StringComparer]::Ordinal
        )
        foreach ($property in $root.EnumerateObject()) {
            if (-not $contract.TryAdd($property.Name, $property.Value)) {
                throw "Portable runtime release contract has duplicate fields."
            }
        }
        $expectedFields = @(
            "appSessionProtocol",
            "contractVersion",
            "currentSchema",
            "minimumPortableSchema",
            "supervisorCapability"
        )
        if ($contract.Count -ne $expectedFields.Count) {
            throw "Portable runtime release contract has unsupported fields."
        }
        foreach ($field in $expectedFields) {
            if (-not $contract.ContainsKey($field)) {
                throw "Portable runtime release contract has unsupported fields."
            }
        }
        $integerFields = @("contractVersion", "supervisorCapability", "minimumPortableSchema", "currentSchema")
        foreach ($field in $integerFields) {
            $value = $contract[$field]
            $raw = $value.GetRawText()
            $parsed = [int64]0
            if (
                $value.ValueKind -ne [System.Text.Json.JsonValueKind]::Number -or
                $raw -notmatch '^(?:0|[1-9][0-9]*)$' -or
                -not $value.TryGetInt64([ref] $parsed)
            ) {
                throw "Portable runtime release contract field $field must be an exact JSON integer."
            }
        }
        $contractVersion = [int64] $contract.contractVersion.GetInt64()
        $supervisorCapability = [int64] $contract.supervisorCapability.GetInt64()
        $appSessionProtocol = if ($contract.appSessionProtocol.ValueKind -eq [System.Text.Json.JsonValueKind]::String) {
            $contract.appSessionProtocol.GetString()
        }
        else {
            $null
        }
        $minimumSchema = [int64] $contract.minimumPortableSchema.GetInt64()
        $currentSchema = [int64] $contract.currentSchema.GetInt64()
        if (
            $appSessionProtocol -isnot [string] -or
            $contractVersion -ne 1 -or
            $supervisorCapability -cne 3 -or
            $appSessionProtocol -cne "renderpilot-portable-app-session-v2" -or
            $minimumSchema -cne 4 -or
            $currentSchema -cne 16
        ) {
            throw "Portable runtime release contract has an unsupported version or range."
        }
        [pscustomobject]@{
            SupervisorCapability = [uint16] $supervisorCapability
            AppSessionProtocol = $appSessionProtocol
            MinimumSchema = [int] $minimumSchema
            CurrentSchema = [int] $currentSchema
        }
    }
    finally {
        $document.Dispose()
    }
}

function New-RenderPilotPortableRpuManifest {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Version,
        [Parameter(Mandatory)] [psobject] $RuntimeRelease,
        [Parameter(Mandatory)] [string] $AppSha256,
        [Parameter(Mandatory)] [Int64] $AppLength
    )

    [ordered]@{
        protocol = "renderpilot-portable-rpu-v1"
        platform = "windows-x86_64-portable"
        version = $Version
        app_sha256 = $AppSha256.ToLowerInvariant()
        app_length = $AppLength
        minimum_supervisor_protocol = $RuntimeRelease.SupervisorCapability
        app_session_protocol = $RuntimeRelease.AppSessionProtocol
        minimum_schema = $RuntimeRelease.MinimumSchema
        maximum_schema = $RuntimeRelease.CurrentSchema
        portable_role = "app"
    }
}

function New-RenderPilotUniqueStagingRoot {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Parent,
        [Parameter(Mandatory)] [string] $Prefix
    )

    if (
        [string]::IsNullOrWhiteSpace($Prefix) -or
        [IO.Path]::GetFileName($Prefix) -ne $Prefix
    ) {
        throw "Staging-root prefix must be one non-empty filename prefix."
    }
    if (-not (Test-Path -LiteralPath $Parent -PathType Container)) {
        throw "Staging-root parent directory was not found: $Parent"
    }
    $parentPath = (Resolve-Path -LiteralPath $Parent).Path
    $root = Join-Path $parentPath ("{0}{1}" -f $Prefix, [Guid]::NewGuid().ToString("N"))
    (New-Item -ItemType Directory -Path $root -ErrorAction Stop).FullName
}

function Copy-RenderPilotFileCreateNew {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Source,
        [Parameter(Mandatory)] [string] $Destination
    )

    $sourceStream = [IO.File]::Open($Source, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
    try {
        $destinationStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try {
            $sourceStream.CopyTo($destinationStream)
            $destinationStream.Flush($true)
        }
        finally {
            $destinationStream.Dispose()
        }
    }
    finally {
        $sourceStream.Dispose()
    }
}

function New-RenderPilotPortableZip {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Source,
        [Parameter(Mandatory)] [string] $Destination,
        [Parameter(Mandatory)] [string] $EntryName
    )

    $destinationStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $archive = [IO.Compression.ZipArchive]::new(
            $destinationStream,
            [IO.Compression.ZipArchiveMode]::Create,
            $true
        )
        try {
            $entry = $archive.CreateEntry($EntryName, [IO.Compression.CompressionLevel]::Optimal)
            $sourceStream = [IO.File]::Open($Source, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
            try {
                $entryStream = $entry.Open()
                try {
                    $sourceStream.CopyTo($entryStream)
                }
                finally {
                    $entryStream.Dispose()
                }
            }
            finally {
                $sourceStream.Dispose()
            }
        }
        finally {
            $archive.Dispose()
        }
        $destinationStream.Flush($true)
    }
    finally {
        $destinationStream.Dispose()
    }
}

function Test-RenderPilotPeVersion {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Path,
        [Parameter(Mandatory)] [string] $ExpectedVersion,
        [Parameter(Mandatory)] [ValidateSet("FileVersionRaw", "ProductVersion")] [string] $Property
    )

    $versionInfo = (Get-Item -LiteralPath $Path -ErrorAction Stop).VersionInfo
    if ($Property -eq "ProductVersion") {
        $actualProductVersion = $versionInfo.ProductVersion
        if ($actualProductVersion -ne $ExpectedVersion) {
            throw "Portable PE ProductVersion '$actualProductVersion' does not match '$ExpectedVersion'."
        }
        return $actualProductVersion
    }

    $expected = [Version]::Parse($ExpectedVersion)
    $actual = $versionInfo.FileVersionRaw
    if (
        $actual.Major -ne $expected.Major -or
        $actual.Minor -ne $expected.Minor -or
        $actual.Build -ne $expected.Build -or
        $actual.Revision -ne 0
    ) {
        throw "Portable PE version $actual does not match release version $expected."
    }
    return $actual
}

function New-RenderPilotPortableRpu {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $App,
        [Parameter(Mandatory)] [string] $ManifestJson,
        [Parameter(Mandatory)] [string] $Destination
    )

    $destinationStream = [IO.File]::Open($Destination, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
    try {
        $archive = [IO.Compression.ZipArchive]::new($destinationStream, [IO.Compression.ZipArchiveMode]::Create, $true)
        try {
            $portableEpoch = [DateTimeOffset]::new(1980, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $manifestEntry = $archive.CreateEntry("rpu-manifest.json", [IO.Compression.CompressionLevel]::NoCompression)
            $manifestEntry.LastWriteTime = $portableEpoch
            $manifestStream = $manifestEntry.Open()
            try {
                $manifestBytes = [Text.UTF8Encoding]::new($false).GetBytes($ManifestJson)
                $manifestStream.Write($manifestBytes, 0, $manifestBytes.Length)
            }
            finally { $manifestStream.Dispose() }

            $appEntry = $archive.CreateEntry("app/renderpilot-app.exe", [IO.Compression.CompressionLevel]::NoCompression)
            $appEntry.LastWriteTime = $portableEpoch
            $appStream = $appEntry.Open()
            $sourceStream = [IO.File]::Open($App, [IO.FileMode]::Open, [IO.FileAccess]::Read, [IO.FileShare]::Read)
            try { $sourceStream.CopyTo($appStream) }
            finally { $sourceStream.Dispose(); $appStream.Dispose() }
        }
        finally { $archive.Dispose() }
        $destinationStream.Flush($true)
    }
    finally { $destinationStream.Dispose() }
}
