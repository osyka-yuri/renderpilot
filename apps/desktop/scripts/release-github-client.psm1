Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$script:GitHubApiVersion = '2022-11-28'
$script:GitHubAcceptHeader = 'application/vnd.github+json'

function Get-RenderPilotGitHubProperty {
    param(
        [Parameter(Mandatory)] $Object,
        [Parameter(Mandatory)] [string] $Name,
        [Parameter(Mandatory)] [string] $Label
    )

    if ($null -eq $Object -or $Object.PSObject.Properties.Name -notcontains $Name) {
        throw "$Label did not include required property '$Name'."
    }
    return $Object.PSObject.Properties[$Name].Value
}

function Get-RenderPilotGitHubSha {
    param(
        [Parameter(Mandatory)] [string] $Value,
        [Parameter(Mandatory)] [string] $Label
    )

    if ($Value -notmatch '^[0-9a-fA-F]{40}$') {
        throw "$Label must be a 40-character Git commit SHA."
    }
    return $Value.ToLowerInvariant()
}

function ConvertTo-RenderPilotGitHubResult {
    param(
        [Parameter(Mandatory)] [Nullable[int]] $StatusCode,
        [AllowNull()] [string] $Content,
        [AllowNull()] [string] $TransportError
    )

    if (-not [string]::IsNullOrEmpty($TransportError)) {
        return [pscustomobject]@{
            StatusCode = $StatusCode
            Succeeded = $false
            Json = $null
            Error = 'GitHub API transport request failed.'
        }
    }
    if ($null -eq $StatusCode) {
        return [pscustomobject]@{
            StatusCode = $null
            Succeeded = $false
            Json = $null
            Error = 'GitHub API response did not include an HTTP status code.'
        }
    }
    try {
        $parsed = $Content | ConvertFrom-Json -ErrorAction Stop
    }
    catch {
        return [pscustomobject]@{
            StatusCode = $StatusCode
            Succeeded = $false
            Json = $null
            Error = 'GitHub API returned malformed JSON.'
        }
    }
    if ($StatusCode -lt 200 -or $StatusCode -ge 300) {
        return [pscustomobject]@{
            StatusCode = $StatusCode
            Succeeded = $false
            Json = $parsed
            Error = "GitHub API returned HTTP $StatusCode."
        }
    }
    return [pscustomobject]@{
        StatusCode = $StatusCode
        Succeeded = $true
        Json = $parsed
        Error = $null
    }
}

function Invoke-RenderPilotGitHubJson {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [ValidateSet('GET', 'PATCH', 'POST')] [string] $Method,
        [Parameter(Mandatory)] [ValidatePattern('^[^/].*$')] [string] $Endpoint,
        [Parameter(Mandatory)] [string] $Token,
        $Body,
        [switch] $Upload,
        [string] $InputFile,
        [scriptblock] $Transport
    )

    if ($PSVersionTable.PSVersion.Major -lt 7) {
        throw 'GitHub release publishing requires PowerShell 7 or later.'
    }
    if ([string]::IsNullOrWhiteSpace($Token)) {
        throw 'GitHub release publishing requires an authenticated token.'
    }
    if ($Upload -and [string]::IsNullOrWhiteSpace($InputFile)) {
        throw 'GitHub upload requests require a local input file.'
    }
    if (-not $Upload -and -not [string]::IsNullOrWhiteSpace($InputFile)) {
        throw 'Only GitHub upload requests may include a local input file.'
    }

    $baseUri = if ($Upload) { 'https://uploads.github.com/' } else { 'https://api.github.com/' }
    $headers = @{
        Accept = $script:GitHubAcceptHeader
        Authorization = "Bearer $Token"
        'X-GitHub-Api-Version' = $script:GitHubApiVersion
    }
    $request = @{
        Uri = "$baseUri$Endpoint"
        Method = $Method
        Headers = $headers
        SkipHttpErrorCheck = $true
        ErrorAction = 'Stop'
    }
    if ($Upload) {
        $request.ContentType = 'application/octet-stream'
        $request.InFile = $InputFile
    }
    elseif ($null -ne $Body) {
        $request.ContentType = 'application/json'
        $request.Body = $Body | ConvertTo-Json -Depth 20 -Compress
    }

    try {
        $response = if ($null -ne $Transport) {
            & $Transport $request
        }
        else {
            Invoke-WebRequest @request
        }
    }
    catch {
        return ConvertTo-RenderPilotGitHubResult -StatusCode $null -Content $null -TransportError $_.Exception.Message
    }

    if ($null -eq $response -or $response.PSObject.Properties.Name -notcontains 'StatusCode') {
        return ConvertTo-RenderPilotGitHubResult -StatusCode $null -Content $null -TransportError $null
    }
    $statusCode = $response.StatusCode
    if ($statusCode -isnot [int] -and $statusCode -isnot [long]) {
        return ConvertTo-RenderPilotGitHubResult -StatusCode $null -Content $null -TransportError $null
    }
    $content = if ($response.PSObject.Properties.Name -contains 'Content') { [string] $response.Content } else { '' }
    return ConvertTo-RenderPilotGitHubResult -StatusCode ([int] $statusCode) -Content $content -TransportError $null
}

function Get-RenderPilotGitHubReleaseByTag {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Repository,
        [Parameter(Mandatory)] [string] $Tag,
        [Parameter(Mandatory)] [string] $Token,
        [scriptblock] $Transport
    )

    $encodedTag = [uri]::EscapeDataString($Tag)
    $result = Invoke-RenderPilotGitHubJson `
        -Method GET `
        -Endpoint "repos/$Repository/releases/tags/$encodedTag" `
        -Token $Token `
        -Transport $Transport
    if ($result.Succeeded) {
        return [pscustomobject]@{ Found = $true; Release = $result.Json; Result = $result }
    }
    if ($result.StatusCode -eq 404 -and $null -ne $result.Json) {
        return [pscustomobject]@{ Found = $false; Release = $null; Result = $result }
    }
    throw "Reading GitHub release tag $Tag failed: $($result.Error)"
}

function Get-RenderPilotGitHubReleaseById {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Repository,
        [Parameter(Mandatory)] [long] $ReleaseId,
        [Parameter(Mandatory)] [string] $Token,
        [scriptblock] $Transport
    )

    $result = Invoke-RenderPilotGitHubJson `
        -Method GET `
        -Endpoint "repos/$Repository/releases/$ReleaseId" `
        -Token $Token `
        -Transport $Transport
    if (-not $result.Succeeded) {
        throw "Reading GitHub release ID $ReleaseId failed: $($result.Error)"
    }
    return $result.Json
}

function Get-RenderPilotGitHubPeeledTagCommit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Repository,
        [Parameter(Mandatory)] [string] $Tag,
        [Parameter(Mandatory)] [string] $Token,
        [scriptblock] $Transport
    )

    $encodedTag = [uri]::EscapeDataString($Tag)
    $result = Invoke-RenderPilotGitHubJson `
        -Method GET `
        -Endpoint "repos/$Repository/git/ref/tags/$encodedTag" `
        -Token $Token `
        -Transport $Transport
    if (-not $result.Succeeded) {
        throw "Reading GitHub tag $Tag failed: $($result.Error)"
    }

    $object = Get-RenderPilotGitHubProperty `
        -Object (Get-RenderPilotGitHubProperty -Object $result.Json -Name 'object' -Label "GitHub tag $Tag") `
        -Name 'type' `
        -Label "GitHub tag $Tag object"
    $sha = Get-RenderPilotGitHubProperty `
        -Object (Get-RenderPilotGitHubProperty -Object $result.Json -Name 'object' -Label "GitHub tag $Tag") `
        -Name 'sha' `
        -Label "GitHub tag $Tag object"
    $objectType = [string] $object
    $objectSha = Get-RenderPilotGitHubSha -Value ([string] $sha) -Label "GitHub tag $Tag object SHA"

    for ($depth = 0; $depth -lt 8; $depth += 1) {
        if ($objectType -eq 'commit') {
            return $objectSha
        }
        if ($objectType -ne 'tag') {
            throw "GitHub tag $Tag resolves to unsupported object type '$objectType'."
        }
        $tagResult = Invoke-RenderPilotGitHubJson `
            -Method GET `
            -Endpoint "repos/$Repository/git/tags/$objectSha" `
            -Token $Token `
            -Transport $Transport
        if (-not $tagResult.Succeeded) {
            throw "Peeling GitHub tag $Tag failed: $($tagResult.Error)"
        }
        $tagObject = Get-RenderPilotGitHubProperty -Object $tagResult.Json -Name 'object' -Label "GitHub annotated tag $Tag"
        $objectType = [string] (Get-RenderPilotGitHubProperty -Object $tagObject -Name 'type' -Label "GitHub annotated tag $Tag object")
        $objectSha = Get-RenderPilotGitHubSha `
            -Value ([string] (Get-RenderPilotGitHubProperty -Object $tagObject -Name 'sha' -Label "GitHub annotated tag $Tag object")) `
            -Label "GitHub annotated tag $Tag object SHA"
    }
    throw "GitHub tag $Tag exceeds the supported annotated-tag peel depth."
}

function Assert-RenderPilotGitHubPeeledTagCommit {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] [string] $Repository,
        [Parameter(Mandatory)] [string] $Tag,
        [Parameter(Mandatory)] [string] $Token,
        [Parameter(Mandatory)] [string] $ExpectedCommit,
        [scriptblock] $Transport
    )

    $expected = Get-RenderPilotGitHubSha -Value $ExpectedCommit -Label 'Expected release commit'
    $actual = Get-RenderPilotGitHubPeeledTagCommit `
        -Repository $Repository `
        -Tag $Tag `
        -Token $Token `
        -Transport $Transport
    if ($actual -cne $expected) {
        throw "Release tag $Tag resolves to $actual, not the initially captured commit $expected."
    }
    return $actual
}

Export-ModuleMember -Function @(
    'Assert-RenderPilotGitHubPeeledTagCommit',
    'Get-RenderPilotGitHubPeeledTagCommit',
    'Get-RenderPilotGitHubReleaseById',
    'Get-RenderPilotGitHubReleaseByTag',
    'Invoke-RenderPilotGitHubJson'
)
