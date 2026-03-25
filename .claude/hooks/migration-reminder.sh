#!/usr/bin/env bash
# Post-edit hook: remind to check migrations when DB model files change
# Reads file path from stdin JSON: tool_input.file_path

INPUT=$(cat)
FILE=$(echo "$INPUT" | jq -r '.tool_input.file_path // empty')

if echo "$FILE" | grep -qE "(models|database|correlator)\.rs$"; then
  echo "REMINDER: If you changed DB models or queries, verify migrations/init.sql is in sync."
fi

exit 0
