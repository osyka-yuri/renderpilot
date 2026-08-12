Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

Import-Module (Join-Path $PSScriptRoot 'release-github-client.psm1') -Force

function Assert-True {
    param(
        [Parameter(Mandatory)] [bool] $Condition,
        [Parameter(Mandatory)] [string] $Message
    )

    if (-not $Condition) {
        throw $Message
    }
}

function Assert-Equal {
    param(
        $Actual,
        $Expected,
        [Parameter(Mandatory)] [string] $Message
    )

    if ($Actual -cne $Expected) {
        throw "$Message Expected '$Expected'; received '$Actual'."
    }
}

function Assert-Throws {
    param(
        [Parameter(Mandatory)] [scriptblock] $Action,
        [Parameter(Mandatory)] [string] $ExpectedMessage
    )

    try {
        & $Action
    }
    catch {
        Assert-True -Condition $_.Exception.Message.Contains($ExpectedMessage) -Message "Unexpected error: $($_.Exception.Message)"
        return
    }
    throw "Expected an error containing '$ExpectedMessage'."
}

function New-MockResponse {
    param(
        [Parameter(Mandatory)] [int] $StatusCode,
        [Parameter(Mandatory)] [string] $Content
    )

    return [pscustomobject]@{
        StatusCode = $StatusCode
        Content = $Content
    }
}

function New-MockTransport {
    param([Parameter(Mandatory)] [object[]] $Responses)

    $queue = [System.Collections.Queue]::new()
    foreach ($response in $Responses) {
        $queue.Enqueue($response)
    }
    return {
        param($Request)

        if ($queue.Count -eq 0) {
            throw 'Mock transport received an unexpected request.'
        }
        $next = $queue.Dequeue()
        if ($next -is [System.Exception]) {
            throw $next
        }
        return $next
    }.GetNewClosure()
}

$repository = 'owner/repository'
$tag = 'v1.9.0'
$token = 'unit-test-token'
$commit = 'a' * 40
$movedCommit = 'b' * 40

# 200 responses parse structured JSON and preserve the fixed REST headers.
$seenRequests = [System.Collections.Generic.List[object]]::new()
$response200 = New-MockResponse -StatusCode 200 -Content '{"id":501}'
$transport200 = {
    param($Request)
    $seenRequests.Add($Request) | Out-Null
    return $response200
}.GetNewClosure()
$ok = Invoke-RenderPilotGitHubJson `
    -Method GET `
    -Endpoint "repos/$repository/releases/tags/$tag" `
    -Token $token `
    -Transport $transport200
Assert-True -Condition $ok.Succeeded -Message 'HTTP 200 must succeed.'
Assert-Equal -Actual $ok.StatusCode -Expected 200 -Message 'HTTP 200 status was not retained.'
Assert-Equal -Actual $ok.Json.id -Expected 501 -Message 'HTTP 200 JSON was not parsed.'
Assert-Equal -Actual $seenRequests[0].Headers['X-GitHub-Api-Version'] -Expected '2022-11-28' -Message 'GitHub API version must be fixed.'
Assert-True -Condition $seenRequests[0].SkipHttpErrorCheck -Message 'GitHub client must inspect HTTP failures itself.'

# A successful create exposes the created release object directly. Callers can
# validate and use this read-your-write response without an eventually
# consistent release-by-tag lookup.
$createdRelease = Invoke-RenderPilotGitHubJson `
    -Method POST `
    -Endpoint "repos/$repository/releases" `
    -Token $token `
    -Body @{ draft = $true } `
    -Transport (New-MockTransport @(
        New-MockResponse `
            -StatusCode 201 `
            -Content '{"id":502,"tag_name":"renderpilot-staging-v1.9.0-42","draft":true}'
    ))
Assert-True -Condition $createdRelease.Succeeded -Message 'HTTP 201 create must succeed.'
Assert-Equal -Actual $createdRelease.StatusCode -Expected 201 -Message 'HTTP 201 status was not retained.'
Assert-Equal -Actual $createdRelease.Json.id -Expected 502 -Message 'Created release JSON was not retained.'

# Only the release-by-tag lookup treats exactly 404 as an absent release.
$missing = Get-RenderPilotGitHubReleaseByTag `
    -Repository $repository `
    -Tag $tag `
    -Token $token `
    -Transport (New-MockTransport @(New-MockResponse -StatusCode 404 -Content '{"message":"Not Found"}'))
Assert-True -Condition (-not $missing.Found) -Message 'HTTP 404 must mean only an absent release tag.'

Assert-Throws -ExpectedMessage 'malformed JSON' -Action {
    Get-RenderPilotGitHubReleaseByTag `
        -Repository $repository `
        -Tag $tag `
        -Token $token `
        -Transport (New-MockTransport @(New-MockResponse -StatusCode 404 -Content '{')) | Out-Null
}

foreach ($statusCode in @(401, 403, 500)) {
    Assert-Throws -ExpectedMessage "HTTP $statusCode" -Action {
        Get-RenderPilotGitHubReleaseByTag `
            -Repository $repository `
            -Tag $tag `
            -Token $token `
            -Transport (New-MockTransport @(New-MockResponse -StatusCode $statusCode -Content '{}')) | Out-Null
    }
}

Assert-Throws -ExpectedMessage 'malformed JSON' -Action {
    Get-RenderPilotGitHubReleaseByTag `
        -Repository $repository `
        -Tag $tag `
        -Token $token `
        -Transport (New-MockTransport @(New-MockResponse -StatusCode 200 -Content '{')) | Out-Null
}

# A create 422 is retry-safe only when the next refetch finds the expected draft.
$createRaceTransport = New-MockTransport @(
    (New-MockResponse -StatusCode 422 -Content '{"message":"already_exists"}'),
    (New-MockResponse -StatusCode 200 -Content '{"id":501,"tag_name":"renderpilot-staging-v1.9.0-42"}')
)
$created = Invoke-RenderPilotGitHubJson `
    -Method POST `
    -Endpoint "repos/$repository/releases" `
    -Token $token `
    -Body @{ draft = $true } `
    -Transport $createRaceTransport
Assert-True -Condition (-not $created.Succeeded -and $created.StatusCode -eq 422) -Message 'Create race must retain HTTP 422 for an exact refetch decision.'
$racedDraft = Get-RenderPilotGitHubReleaseByTag `
    -Repository $repository `
    -Tag 'renderpilot-staging-v1.9.0-42' `
    -Token $token `
    -Transport $createRaceTransport
Assert-True -Condition ($racedDraft.Found -and $racedDraft.Release.id -eq 501) -Message 'A 422 create race must refetch the created draft.'

# An exact final retry is a read-only successful GET; the publisher's manifest
# policy separately verifies its metadata, assets, and tag commit.
$retry = Get-RenderPilotGitHubReleaseByTag `
    -Repository $repository `
    -Tag $tag `
    -Token $token `
    -Transport (New-MockTransport @(New-MockResponse -StatusCode 200 -Content '{"id":501}'))
Assert-True -Condition ($retry.Found -and $retry.Release.id -eq 501) -Message 'Exact final retry must retain the fetched release for verification.'

# The remote peeled tag is captured before PATCH and must match again after it.
$tagRef = "{`"object`":{`"type`":`"commit`",`"sha`":`"$commit`"}}"
$capturedCommit = Assert-RenderPilotGitHubPeeledTagCommit `
    -Repository $repository `
    -Tag $tag `
    -Token $token `
    -ExpectedCommit $commit `
    -Transport (New-MockTransport @(New-MockResponse -StatusCode 200 -Content $tagRef))
Assert-Equal -Actual $capturedCommit -Expected $commit -Message 'Initial peeled tag commit was not captured.'
$movedTagRef = "{`"object`":{`"type`":`"commit`",`"sha`":`"$movedCommit`"}}"
Assert-Throws -ExpectedMessage 'not the initially captured commit' -Action {
    Assert-RenderPilotGitHubPeeledTagCommit `
        -Repository $repository `
        -Tag $tag `
        -Token $token `
        -ExpectedCommit $capturedCommit `
        -Transport (New-MockTransport @(New-MockResponse -StatusCode 200 -Content $movedTagRef)) | Out-Null
}

Write-Output 'release-github-client tests passed'
