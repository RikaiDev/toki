#!/bin/bash
# Claude Code PostToolUse hook
# Automatically records outcomes (commits, issues, PRs) during sessions

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
    fi
fi

# Detect gh issue close
if echo "$TOOL_INPUT" | grep -qE "gh issue close"; then
    # Extract issue number from command
    ISSUE_NUM=$(echo "$TOOL_INPUT" | grep -oE 'gh issue close [0-9]+' | grep -oE '[0-9]+')

    if [ -n "$ISSUE_NUM" ]; then
        record_outcome "issue_closed" "#$ISSUE_NUM" "Closed issue #$ISSUE_NUM"
    fi
fi

# Detect gh pr create
if echo "$TOOL_INPUT" | grep -qE "gh pr create"; then
    # Try to extract PR URL from result
    PR_URL=$(echo "$TOOL_RESULT" | grep -oE 'https://github.com/[^/]+/[^/]+/pull/[0-9]+' | head -1)

    if [ -n "$PR_URL" ]; then
        PR_NUM=$(echo "$PR_URL" | grep -oE '[0-9]+$')
        record_outcome "pr_created" "#$PR_NUM" "Created PR #$PR_NUM"
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

# Detect git push (informational, not an outcome type but useful context)
# We skip this as it's not a direct outcome

exit 0
