#!/usr/bin/env bash
# Hook: Auto-format Rust files after edits
# Triggered by: PostToolUse (Edit|Write)

set -e

INPUT=$(cat)
FILE_PATH=$(echo "${INPUT}" | jq -r '.tool_input.file_path // empty')

if [[ ${FILE_PATH} == *.rs || ${FILE_PATH} == *Cargo.toml ]]; then
	if command -v cargo &>/dev/null; then
		cargo fmt --all 2>/dev/null || true
	fi
fi

exit 0
