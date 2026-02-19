#!/bin/sh
# Conformance test runner for thaum.
#
# Usage: echo 'script' | conformance_runner.sh <shell>
#   <shell> is "dash" or "bash-posix"
#
# Reads a script from stdin, executes it with the specified shell,
# and outputs a simple format:
#   EXIT:<exit_code>
#   STDOUT:<base64_encoded_stdout>
#   STDERR:<base64_encoded_stderr>

set -e

SHELL_NAME="${1:-dash}"

# Read script from stdin
SCRIPT=$(cat)

# Create a temp file for the script
TMPSCRIPT=$(mktemp /tmp/conformance.XXXXXX)
printf '%s' "$SCRIPT" > "$TMPSCRIPT"

# Create temp files for output capture
TMPOUT=$(mktemp /tmp/stdout.XXXXXX)
TMPERR=$(mktemp /tmp/stderr.XXXXXX)

# Execute with the appropriate shell
EXIT_CODE=0
case "$SHELL_NAME" in
    dash)
        dash "$TMPSCRIPT" >"$TMPOUT" 2>"$TMPERR" || EXIT_CODE=$?
        ;;
    bash-posix)
        bash --posix "$TMPSCRIPT" >"$TMPOUT" 2>"$TMPERR" || EXIT_CODE=$?
        ;;
    *)
        echo "Unknown shell: $SHELL_NAME" >&2
        exit 1
        ;;
esac

# Output results
# Use base64 to safely encode binary output
STDOUT_B64=$(base64 < "$TMPOUT")
STDERR_B64=$(base64 < "$TMPERR")

printf 'EXIT:%d\n' "$EXIT_CODE"
printf 'STDOUT:%s\n' "$STDOUT_B64"
printf 'STDERR:%s\n' "$STDERR_B64"

# Cleanup
rm -f "$TMPSCRIPT" "$TMPOUT" "$TMPERR"
