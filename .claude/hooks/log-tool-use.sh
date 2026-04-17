#!/usr/bin/env bash
# Post-tool hook: log every tool use to .claude-tool-logs/YYYY-MM-DD.jsonl
# Reads from stdin JSON: tool_name, tool_input, session_id

DIR=".claude-tool-logs"
mkdir -p "$DIR"

LOG="$DIR/$(date +%Y-%m-%d).jsonl"

INPUT=$(cat)

echo "$INPUT" | jq -c \
  --arg ts "$(date -u +%Y-%m-%dT%H:%M:%SZ)" \
  '{ts:$ts,tool:.tool_name,input:.tool_input,session:.session_id}' >> "$LOG"
