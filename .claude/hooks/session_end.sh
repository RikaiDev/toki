#!/bin/bash
# Claude Code SessionEnd hook
# Called when a Claude Code session ends

# Read JSON input from stdin
INPUT=$(cat)

# Extract fields from JSON
SESSION_ID=$(echo "$INPUT" | grep -o '"session_id":"[^"]*"' | cut -d'"' -f4)
REASON=$(echo "$INPUT" | grep -o '"reason":"[^"]*"' | cut -d'"' -f4)

if [ -z "$SESSION_ID" ]; then
    echo "Error: No session_id in input" >&2
    exit 0  # Don't block on error
fi

# Get the project directory from environment
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"

# End the session (silently - hooks should be quiet)
if [ -n "$REASON" ]; then
    "$PROJECT_DIR/target/release/toki" session end --id "$SESSION_ID" --reason "$REASON" 2>/dev/null
else
    "$PROJECT_DIR/target/release/toki" session end --id "$SESSION_ID" 2>/dev/null
fi

exit 0
