#!/usr/bin/env python3
"""Adds a "Claude Usage" tool to iTerm2's toolbelt, next to Session Status.

The panel is a web view of http://127.0.0.1:47821/panel, served by the Claude
Usage app, which does all the polling. This script only registers the tool, so
the app must be running for the panel to show data; if it isn't listening when
iTerm2 starts, the script tries to launch it in the background.

Install: run integrations/iterm2/install.sh (links this file into iTerm2's
AutoLaunch folder), then View > Toolbelt > Claude Usage.
Requires iTerm2 > Settings > General > Magic > Enable Python API, and the
Python runtime (Scripts > Manage > Install Python Runtime).
"""

import os
import socket
import subprocess

import iterm2

PORT = int(os.environ.get("CLAUDE_USAGE_PANEL_PORT", "47821"))
URL = f"http://127.0.0.1:{PORT}/panel"
BUNDLE_ID = "com.phase2online.claude-usage"


def app_listening() -> bool:
    try:
        with socket.create_connection(("127.0.0.1", PORT), timeout=0.5):
            return True
    except OSError:
        return False


async def main(connection):
    if not app_listening():
        # -g: don't steal focus. Fails quietly if the app isn't installed.
        subprocess.run(["open", "-g", "-b", BUNDLE_ID], check=False,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    await iterm2.tool.async_register_web_view_tool(
        connection,
        display_name="Claude Usage",
        identifier=f"{BUNDLE_ID}.panel",
        reveal_if_already_registered=False,
        url=URL,
    )


# The tool stays registered for as long as this script's connection is open.
iterm2.run_forever(main)
