#!/bin/bash
# Claude Code PostToolUse hook
# Automatically records outcomes (commits, issues, PRs) and links issues to sessions

# Read JSON input from stdin
INPUT=$(cat)

# Extract fields from JSON
TOOL_NAME=$(echo "$INPUT" | grep -o '"tool_name":"[^"]*"' | cut -d'"' -f4)
TOOL_INPUT=$(echo "$INPUT" | grep -o '"tool_input":"[^"]*"' | cut -d'"' -f4 | sed 's/\\n/ /g')
TOOL_RESULT=$(echo "$INPUT" | grep -o '"tool_result":"[^"]*"' | head -1 | cut -d'"' -f4 | sed 's/\\n/ /g')
SESSION_ID=$(echo "$INPUT" | grep -o '"session_id":"[^"]*"' | cut -d'"' -f4)

# Only process Bash tool calls
if [ "$TOOL_NAME" != "Bash" ]; then
    exit 0
fi

# Get project directory and toki path
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"
TOKI="$PROJECT_DIR/target/release/toki"

# Check if toki binary exists
if [ ! -x "$TOKI" ]; then
    exit 0
fi

# Check if we have a session
if [ -z "$SESSION_ID" ]; then
    exit 0
fi

# Function to record an outcome
record_outcome() {
    local type="$1"
    local reference="$2"
    local description="$3"

    if [ -n "$reference" ]; then
        "$TOKI" session outcome --id "$SESSION_ID" --outcome-type "$type" --reference "$reference" --description "$description" 2>/dev/null
    else
        "$TOKI" session outcome --id "$SESSION_ID" --outcome-type "$type" --description "$description" 2>/dev/null
    fi
}

# Function to link an issue to the session
link_issue() {
    local issue_id="$1"
    local system="${2:-github}"
    local relationship="${3:-worked_on}"

    "$TOKI" session link --id "$SESSION_ID" --issue "$issue_id" --system "$system" --relationship "$relationship" 2>/dev/null
}

# Function to extract and link issue references from text
# Supports: #123, fixes #123, closes #123, GH-123, etc.
extract_and_link_issues() {
    local text="$1"
    local relationship="${2:-referenced}"

    # Extract issue numbers with # prefix (most common)
    local issues=$(echo "$text" | grep -oE '#[0-9]+' | grep -oE '[0-9]+' | sort -u)

    for issue in $issues; do
        # Check if this is a "closes" or "fixes" reference
        if echo "$text" | grep -qiE "(closes?|fixes?|resolves?)[[:space:]]*#$issue"; then
            link_issue "$issue" "github" "closed"
        else
            link_issue "$issue" "github" "$relationship"
        fi
    done
}

# Detect git commit
if echo "$TOOL_INPUT" | grep -qE "git commit"; then
    # Try to extract commit hash from result (format: [branch hash] message)
    COMMIT_HASH=$(echo "$TOOL_RESULT" | grep -oE '\[[a-zA-Z0-9_/-]+ [a-f0-9]{7,}\]' | head -1 | grep -oE '[a-f0-9]{7,}')

    if [ -n "$COMMIT_HASH" ]; then
        # Extract commit message (first line after the hash info)
        COMMIT_MSG=$(echo "$TOOL_RESULT" | grep -oE '\] .*$' | head -1 | sed 's/\] //')
        # Truncate message if too long
        COMMIT_MSG="${COMMIT_MSG:0:100}"
        record_outcome "commit" "$COMMIT_HASH" "$COMMIT_MSG"

        # Extract issue references from commit message and tool input
        # Check both the commit message in the result and the original input
        extract_and_link_issues "$COMMIT_MSG" "worked_on"
        extract_and_link_issues "$TOOL_INPUT" "worked_on"
    fi
fi

# Detect gh issue close
if echo "$TOOL_INPUT" | grep -qE "gh issue close"; then
    # Extract issue number from command
    ISSUE_NUM=$(echo "$TOOL_INPUT" | grep -oE 'gh issue close [0-9]+' | grep -oE '[0-9]+')

    if [ -n "$ISSUE_NUM" ]; then
        record_outcome "issue_closed" "#$ISSUE_NUM" "Closed issue #$ISSUE_NUM"
        link_issue "$ISSUE_NUM" "github" "closed"
    fi
fi

# Detect gh issue view (working on an issue)
if echo "$TOOL_INPUT" | grep -qE "gh issue view"; then
    # Extract issue number from command
    ISSUE_NUM=$(echo "$TOOL_INPUT" | grep -oE 'gh issue view [0-9]+' | grep -oE '[0-9]+')

    if [ -n "$ISSUE_NUM" ]; then
        link_issue "$ISSUE_NUM" "github" "worked_on"
    fi
fi

# Detect gh issue create
if echo "$TOOL_INPUT" | grep -qE "gh issue create"; then
    # Try to extract issue URL from result
    ISSUE_URL=$(echo "$TOOL_RESULT" | grep -oE 'https://github.com/[^/]+/[^/]+/issues/[0-9]+' | head -1)

    if [ -n "$ISSUE_URL" ]; then
        ISSUE_NUM=$(echo "$ISSUE_URL" | grep -oE '[0-9]+$')
        link_issue "$ISSUE_NUM" "github" "worked_on"
    fi
fi

# Detect gh pr create
if echo "$TOOL_INPUT" | grep -qE "gh pr create"; then
    # Try to extract PR URL from result
    PR_URL=$(echo "$TOOL_RESULT" | grep -oE 'https://github.com/[^/]+/[^/]+/pull/[0-9]+' | head -1)

    if [ -n "$PR_URL" ]; then
        PR_NUM=$(echo "$PR_URL" | grep -oE '[0-9]+$')
        record_outcome "pr_created" "#$PR_NUM" "Created PR #$PR_NUM"

        # Extract issue references from the PR body/title in tool input
        extract_and_link_issues "$TOOL_INPUT" "worked_on"
    fi
fi

# Detect gh pr merge
if echo "$TOOL_INPUT" | grep -qE "gh pr merge"; then
    # Extract PR number from command
    PR_NUM=$(echo "$TOOL_INPUT" | grep -oE 'gh pr merge [0-9]+' | grep -oE '[0-9]+')

    if [ -n "$PR_NUM" ]; then
        record_outcome "pr_merged" "#$PR_NUM" "Merged PR #$PR_NUM"
    fi
fi

exit 0
