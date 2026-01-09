#!/usr/bin/env pwsh
# Checkout a GitHub PR for inspection with jj
# Usage: ./scripts/jj-checkout-pr.ps1 https://github.com/owner/repo/pull/123
#    or: ./scripts/jj-checkout-pr.ps1 123  (uses current repo)

param(
    [Parameter(Mandatory = $true, Position = 0)]
    [string]$PrRef
)

# Parse PR URL or number
if ($PrRef -match '^https://github\.com/([^/]+)/([^/]+)/pull/(\d+)$') {
    $owner = $Matches[1]
    $repo = $Matches[2]
    $prNumber = $Matches[3]
}
elseif ($PrRef -match '^\d+$') {
    # Just a number - try to get owner/repo from git remote
    $originUrl = jj git remote list 2>$null | Where-Object { $_ -match '^origin\s+' } | ForEach-Object { ($_ -split '\s+')[1] }
    if ($originUrl -match 'github\.com[:/]([^/]+)/([^/\.]+)') {
        $owner = $Matches[1]
        $repo = $Matches[2]
        $prNumber = $PrRef
    }
    else {
        Write-Error "Could not determine repository from origin remote. Please provide full PR URL."
        exit 1
    }
}
else {
    Write-Error "Invalid PR reference. Use a full URL or PR number."
    exit 1
}

Write-Host "Fetching PR #$prNumber from $owner/$repo..." -ForegroundColor Cyan

# Fetch PR info from GitHub API
try {
    $prData = Invoke-RestMethod -Uri "https://api.github.com/repos/$owner/$repo/pulls/$prNumber" -Headers @{ "User-Agent" = "jj-checkout-pr" }
}
catch {
    Write-Error "Failed to fetch PR data: $_"
    exit 1
}

$forkOwner = $prData.head.user.login
$forkUrl = $prData.head.repo.clone_url
$branchName = $prData.head.ref

Write-Host "PR: $($prData.title)" -ForegroundColor Green
Write-Host "From: $forkOwner/$branchName" -ForegroundColor Gray

# Check if remote already exists
$existingRemotes = jj git remote list 2>$null
$remoteExists = $existingRemotes | Where-Object { $_ -match "^$forkOwner\s+" }

if (-not $remoteExists) {
    Write-Host "Adding remote '$forkOwner'..." -ForegroundColor Yellow
    jj git remote add $forkOwner $forkUrl
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Failed to add remote"
        exit 1
    }
}

# Fetch from the fork
Write-Host "Fetching from '$forkOwner'..." -ForegroundColor Yellow
jj git fetch --remote $forkOwner
if ($LASTEXITCODE -ne 0) {
    Write-Error "Failed to fetch from remote"
    exit 1
}

# Track the branch
$trackName = "$branchName@$forkOwner"
Write-Host "Tracking bookmark '$trackName'..." -ForegroundColor Yellow
jj bookmark track $trackName 2>$null
# Ignore errors - might already be tracked

Write-Host "`nDone! The PR branch is now available." -ForegroundColor Green
Write-Host "View it with: jj log -r '$branchName@$forkOwner'" -ForegroundColor Cyan
Write-Host "Check it out with: jj new $branchName@$forkOwner" -ForegroundColor Cyan
