#!/bin/sh
# Package and install the sh-yaml VS Code extension.
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXT_DIR="$SCRIPT_DIR/../.vscode/extensions/sh-yaml"

cd "$EXT_DIR"
npx @vscode/vsce package --allow-missing-repository --out /tmp/sh-yaml.vsix
code --install-extension /tmp/sh-yaml.vsix
rm /tmp/sh-yaml.vsix

echo "Done. Reload VS Code to activate."
