#!/usr/bin/env fish
# Checkout a GitHub PR for inspection with jj
# Usage: ./scripts/jj-checkout-pr.fish https://github.com/owner/repo/pull/123
#    or: ./scripts/jj-checkout-pr.fish 123  (uses current repo)

if test (count $argv) -lt 1
    echo "Usage: $0 <PR-URL or PR-number>" >&2
    exit 1
end

set pr_ref $argv[1]

# Parse PR URL or number
if string match -qr '^https://github\.com/[^/]+/[^/]+/pull/[0-9]+$' $pr_ref
    set owner (string replace -r 'https://github\.com/([^/]+)/([^/]+)/pull/([0-9]+)' '$1' $pr_ref)
    set repo (string replace -r 'https://github\.com/([^/]+)/([^/]+)/pull/([0-9]+)' '$2' $pr_ref)
    set pr_number (string replace -r 'https://github\.com/([^/]+)/([^/]+)/pull/([0-9]+)' '$3' $pr_ref)
else if string match -qr '^[0-9]+$' $pr_ref
    # Just a number - try to get owner/repo from git remote
    set origin_url (jj git remote list 2>/dev/null | string match -r '^origin\s+(.*)' | tail -1)
    if string match -qr 'github\.com[:/][^/]+/[^/]+' $origin_url
        set owner (string replace -r '.*github\.com[:/]([^/]+)/([^/.]+).*' '$1' $origin_url)
        set repo (string replace -r '.*github\.com[:/]([^/]+)/([^/.]+).*' '$2' $origin_url)
        set pr_number $pr_ref
    else
        echo "Error: Could not determine repository from origin remote. Please provide full PR URL." >&2
        exit 1
    end
else
    echo "Error: Invalid PR reference. Use a full URL or PR number." >&2
    exit 1
end

echo "Fetching PR #$pr_number from $owner/$repo..."

# Fetch PR info from GitHub API
set pr_json (curl -sf -H "User-Agent: jj-checkout-pr" \
    "https://api.github.com/repos/$owner/$repo/pulls/$pr_number")
or begin
    echo "Error: Failed to fetch PR data" >&2
    exit 1
end

# Parse JSON - extract fields from head object
# The head.user.login is the fork owner, head.repo.clone_url is the URL, head.ref is the branch
set fork_owner (echo $pr_json | string match -r '"head":\s*\{[^}]*"user":\s*\{[^}]*"login":\s*"([^"]*)"' | tail -1)
set fork_url (echo $pr_json | string match -r '"head":\s*\{[^}]*"repo":\s*\{[^}]*"clone_url":\s*"([^"]*)"' | tail -1)
set branch_name (echo $pr_json | string match -r '"head":\s*\{[^}]*"ref":\s*"([^"]*)"' | tail -1)
set pr_title (echo $pr_json | string match -r '"title":\s*"([^"]*)"' | tail -1)

echo "PR: $pr_title"
echo "From: $fork_owner/$branch_name"

# Check if remote already exists
if not jj git remote list 2>/dev/null | string match -q "$fork_owner *"
    echo "Adding remote '$fork_owner'..."
    jj git remote add $fork_owner $fork_url
end

# Fetch from the fork
echo "Fetching from '$fork_owner'..."
jj git fetch --remote $fork_owner

# Track the branch
set track_name "$branch_name@$fork_owner"
echo "Tracking bookmark '$track_name'..."
jj bookmark track $track_name 2>/dev/null; or true

echo ""
echo "Done! The PR branch is now available."
echo "View it with: jj log -r '$branch_name@$fork_owner'"
echo "Check it out with: jj new $branch_name@$fork_owner"
