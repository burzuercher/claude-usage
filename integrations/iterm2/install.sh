#!/bin/sh
# Link the Claude Usage toolbelt script into iTerm2's AutoLaunch folder so it
# starts with iTerm2. Re-running is safe.
set -e
src="$(cd "$(dirname "$0")" && pwd)/claude_usage_panel.py"
dir="$HOME/Library/Application Support/iTerm2/Scripts/AutoLaunch"
mkdir -p "$dir"
ln -sf "$src" "$dir/claude_usage_panel.py"
echo "Linked into $dir"
echo "Next: in iTerm2, Scripts > AutoLaunch > claude_usage_panel.py (or restart iTerm2),"
echo "then View > Toolbelt > Claude Usage."
