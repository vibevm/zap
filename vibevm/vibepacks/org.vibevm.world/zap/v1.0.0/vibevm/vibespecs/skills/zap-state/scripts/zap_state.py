#!/usr/bin/env python3
"""Compatibility entrypoint for the maintained ZAP reference data kernel.

The implementation lives in :mod:`zaplib`. Existing runpy callers continue to
receive the original helper names from this module's global namespace.
"""
from __future__ import annotations

from pathlib import Path
import sys

_SCRIPT_DIR = str(Path(__file__).resolve().parent)
if _SCRIPT_DIR not in sys.path:
    sys.path.insert(0, _SCRIPT_DIR)

from zaplib import *  # noqa: F401,F403,E402 - deliberate compatibility surface


if __name__ == "__main__":
    sys.exit(main())
