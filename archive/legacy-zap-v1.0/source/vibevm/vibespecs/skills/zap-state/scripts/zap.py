#!/usr/bin/env python3
"""Public ZAP adaptive campaign CLI."""
from __future__ import annotations

from pathlib import Path
import sys

_SCRIPT_DIR = str(Path(__file__).resolve().parent)
if _SCRIPT_DIR not in sys.path:
    sys.path.insert(0, _SCRIPT_DIR)

from zaplib.application_cli import main  # noqa: E402


if __name__ == "__main__":
    sys.exit(main())
