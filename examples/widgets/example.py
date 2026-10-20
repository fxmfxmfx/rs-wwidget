#!/usr/bin/env python3
"""Compatibility entry point: replace Python with the native example process."""

import os
import sys

if __name__ == "__main__":
    executable = os.path.join(os.path.dirname(os.path.abspath(__file__)), "cpu-monitor")
    try:
        os.execv(executable, [executable, *sys.argv[1:]])
    except OSError as error:
        sys.exit(f"Cannot start native CPU widget: {error}. Run ./build.sh first.")
