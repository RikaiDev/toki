#!/bin/bash
# Claude Code SessionStart hook
# Called when a Claude Code session starts or resumes

# Read JSON input from stdin
INPUT=$(cat)

# Extract session_id from JSON
SESSION_ID=$(echo "$INPUT" | grep -o '"session_id":"[^"]*"' | cut -d'"' -f4)

if [ -z "$SESSION_ID" ]; then
    echo "Error: No session_id in input" >&2
    exit 0  # Don't block on error
fi

# Get the project directory from environment
PROJECT_DIR="${CLAUDE_PROJECT_DIR:-$(pwd)}"

# Start the session (silently - hooks should be quiet)
"$PROJECT_DIR/target/release/toki" session start --id "$SESSION_ID" --project "$PROJECT_DIR" 2>/dev/null

exit 0
