#!/usr/bin/env python3
"""Locate an installed ZAP package runtime and execute its public CLI."""
from __future__ import annotations

import argparse
import os
from pathlib import Path
import subprocess
import sys


def _runtime(root: Path) -> Path | None:
    direct = root / "vibevm" / "vibespecs" / "skills" / "zap-state" / "scripts" / "zap.py"
    if direct.is_file():
        return direct.resolve()
    projected = root / "zap-state" / "scripts" / "zap.py"
    if projected.is_file():
        return projected.resolve()
    return None


def locate(package_root: str | None = None) -> Path:
    if package_root:
        result = _runtime(Path(package_root).resolve())
        if result is None:
            raise SystemExit("explicit --package-root does not contain the ZAP runtime")
        return result
    candidates = set()
    env_root = os.environ.get("ZAP_PACKAGE_ROOT")
    if env_root and (candidate := _runtime(Path(env_root).resolve())):
        candidates.add(candidate)
    script = Path(__file__).resolve()
    for parent in script.parents:
        if (candidate := _runtime(parent)):
            candidates.add(candidate)
        sibling = parent.parent / "zap-state" / "scripts" / "zap.py"
        if sibling.is_file():
            candidates.add(sibling.resolve())
    cwd = Path.cwd().resolve()
    for parent in (cwd, *cwd.parents):
        if (candidate := _runtime(parent)):
            candidates.add(candidate)
        slot = parent / "vibedeps" / "org.vibevm.world" / "zap" / "1.0.0"
        if (candidate := _runtime(slot)):
            candidates.add(candidate)
    if len(candidates) != 1:
        visible = ", ".join(str(path) for path in sorted(candidates))
        raise SystemExit(
            "ZAP runtime location is missing or ambiguous; pass --package-root"
            + (f" (found: {visible})" if visible else "")
        )
    return next(iter(candidates))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-root")
    parser.add_argument("args", nargs=argparse.REMAINDER)
    parsed = parser.parse_args(argv)
    args = parsed.args[1:] if parsed.args[:1] == ["--"] else parsed.args
    runtime = locate(parsed.package_root)
    return subprocess.call([sys.executable, "-B", str(runtime), *args])


if __name__ == "__main__":
    raise SystemExit(main())
