"""Private durable I/O, path, environment, and process-identity helpers."""
from __future__ import annotations

from contextlib import contextmanager
import os
from pathlib import Path
import re
import stat
import time
from typing import Any, Iterator
import uuid

from .common import Refusal, need, packed, parse, sha


ENVIRONMENT_NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_]*$")
_CREDENTIAL_WORDS = ("PASSWORD", "PASSWD", "SECRET", "TOKEN", "CREDENTIAL", "AUTHORIZATION", "API_KEY", "ACCESS_KEY")
_CREDENTIAL_TOKENS = {"PAT", "COOKIE", "BEARER"}


def now_ns() -> int:
    return time.time_ns()


def _is_link_or_reparse(path: Path) -> bool:
    info = path.lstat()
    return stat.S_ISLNK(info.st_mode) or bool(
        getattr(info, "st_file_attributes", 0)
        & getattr(stat, "FILE_ATTRIBUTE_REPARSE_POINT", 0)
    )


def reject_link_components(path: Path) -> None:
    absolute = path.absolute()
    for part in (absolute, *absolute.parents):
        if part.exists() or part.is_symlink():
            need(not _is_link_or_reparse(part), "PATH", f"symlink/reparse path refused: {part}")


def private_root(path: str | os.PathLike[str]) -> Path:
    result = Path(path).absolute()
    reject_link_components(result)
    result.mkdir(parents=True, exist_ok=True)
    reject_link_components(result)
    need(result.is_dir(), "PATH", "transport root is not a directory")
    try:
        result.chmod(0o700)
    except OSError:
        pass
    return result.resolve(strict=True)


def existing_directory(path: str | os.PathLike[str]) -> Path:
    result = Path(path).absolute()
    reject_link_components(result)
    need(result.exists() and result.is_dir(), "PATH", f"directory does not exist: {result}")
    result = result.resolve(strict=True)
    reject_link_components(result)
    return result


def confined_directory(path: str | os.PathLike[str], roots: tuple[Path, ...]) -> Path:
    result = existing_directory(path)
    need(any(result == root or result.is_relative_to(root) for root in roots), "PATH", "working directory is outside allowed workspace roots")
    return result


def atomic_write(path: Path, raw: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.parent / f".{path.name}.{uuid.uuid4().hex}.tmp"
    descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    try:
        with os.fdopen(descriptor, "wb", closefd=True) as stream:
            descriptor = -1
            stream.write(raw)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        try:
            path.chmod(0o600)
        except OSError:
            pass
        if os.name != "nt":
            parent = os.open(path.parent, os.O_RDONLY)
            try:
                os.fsync(parent)
            finally:
                os.close(parent)
    finally:
        if descriptor >= 0:
            os.close(descriptor)
        try:
            temporary.unlink()
        except FileNotFoundError:
            pass


def atomic_json(path: Path, value: Any) -> None:
    atomic_write(path, packed(value) + b"\n")


def publish_file(source: Path, destination: Path) -> None:
    """Publish one already-flushed private artifact under its final name."""
    os.replace(source, destination)
    try:
        destination.chmod(0o600)
    except OSError:
        pass
    if os.name != "nt":
        parent = os.open(destination.parent, os.O_RDONLY)
        try:
            os.fsync(parent)
        finally:
            os.close(parent)


def read_json(path: Path) -> Any:
    try:
        return parse(path.read_bytes())
    except (OSError, UnicodeError, ValueError) as exc:
        if isinstance(exc, Refusal):
            raise
        raise Refusal("TRANSPORT_CORRUPT", f"cannot read transport record {path.name}") from exc


def artifact(path: Path) -> dict[str, Any]:
    raw = path.read_bytes()
    return {"path": str(path), "bytes": len(raw), "sha256": sha(raw)}


def verify_artifact(record: Any, root: Path, *, expected_path: Path | None = None) -> bool:
    if not isinstance(record, dict) or set(record) != {"path", "bytes", "sha256"}:
        return False
    try:
        raw_path = Path(record["path"]).absolute()
        reject_link_components(raw_path)
        path = raw_path.resolve(strict=True)
        if not path.is_relative_to(root) or not path.is_file():
            return False
        if expected_path is not None:
            expected = expected_path.absolute().resolve(strict=True)
            if path != expected:
                return False
        raw = path.read_bytes()
        return type(record["bytes"]) is int and record["bytes"] == len(raw) and record["sha256"] == sha(raw)
    except (OSError, TypeError, ValueError):
        return False


def protected_environment_name(name: str) -> bool:
    upper = name.upper()
    if any(word in upper for word in _CREDENTIAL_WORDS):
        return True
    tokens = set(upper.split("_"))
    if tokens & _CREDENTIAL_TOKENS or "PRIVATE_KEY" in upper:
        return True
    return upper.startswith("ZAP_") and any(word in upper for word in ("OWNER", "CONTROL", "COORDINATOR"))


def validate_environment_name(name: Any) -> str:
    need(isinstance(name, str) and ENVIRONMENT_NAME.fullmatch(name) is not None, "ENVIRONMENT", "invalid environment name")
    need(not protected_environment_name(name), "CREDENTIAL", "credential environment names are not allowed in worker input")
    need(name.upper() != "ZAP_STOP_FILE", "ENVIRONMENT", "ZAP_STOP_FILE is transport-owned")
    return name


@contextmanager
def file_lock(path: Path, timeout: float = 10.0) -> Iterator[None]:
    """A process-safe advisory lock; the durable lock file is never stolen."""
    path.parent.mkdir(parents=True, exist_ok=True)
    stream = path.open("a+b")
    stream.seek(0, os.SEEK_END)
    if stream.tell() == 0:
        stream.write(b"\0")
        stream.flush()
    deadline = time.monotonic() + timeout
    locked = False
    try:
        while not locked:
            try:
                stream.seek(0)
                if os.name == "nt":
                    import msvcrt

                    msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                else:
                    import fcntl

                    fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
                locked = True
            except OSError as exc:
                if time.monotonic() >= deadline:
                    raise Refusal("BUSY", "transport lock is busy") from exc
                time.sleep(0.02)
        yield
    finally:
        if locked:
            stream.seek(0)
            if os.name == "nt":
                import msvcrt

                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl

                fcntl.flock(stream.fileno(), fcntl.LOCK_UN)
        stream.close()


def process_token(pid: int) -> str | None:
    """Return an OS process-start identity, never a PID-derived substitute."""
    if type(pid) is not int or pid <= 0:
        return None
    if os.name == "nt":
        try:
            import ctypes
            from ctypes import wintypes

            query = 0x1000
            kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
            kernel32.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
            kernel32.OpenProcess.restype = wintypes.HANDLE
            kernel32.GetProcessTimes.argtypes = (
                wintypes.HANDLE,
                ctypes.POINTER(wintypes.FILETIME),
                ctypes.POINTER(wintypes.FILETIME),
                ctypes.POINTER(wintypes.FILETIME),
                ctypes.POINTER(wintypes.FILETIME),
            )
            kernel32.GetProcessTimes.restype = wintypes.BOOL
            kernel32.CloseHandle.argtypes = (wintypes.HANDLE,)
            kernel32.CloseHandle.restype = wintypes.BOOL
            handle = kernel32.OpenProcess(query, False, pid)
            if not handle:
                return None
            created = wintypes.FILETIME()
            exited = wintypes.FILETIME()
            kernel = wintypes.FILETIME()
            user = wintypes.FILETIME()
            try:
                if not kernel32.GetProcessTimes(handle, ctypes.byref(created), ctypes.byref(exited), ctypes.byref(kernel), ctypes.byref(user)):
                    return None
                value = (created.dwHighDateTime << 32) | created.dwLowDateTime
                return f"windows-filetime:{value}"
            finally:
                kernel32.CloseHandle(handle)
        except (AttributeError, OSError, ValueError):
            return None
    proc = Path(f"/proc/{pid}/stat")
    if proc.exists():
        try:
            raw = proc.read_text(encoding="ascii")
            fields = raw[raw.rfind(")") + 2 :].split()
            return f"linux-starttime:{fields[19]}"
        except (OSError, IndexError, ValueError):
            return None
    return None


def process_matches(pid: Any, token: Any) -> bool | None:
    if type(pid) is not int or pid <= 0 or not isinstance(token, str) or not token:
        return None
    current = process_token(pid)
    return current == token if current is not None else False


def creation_options() -> dict[str, Any]:
    if os.name == "nt":
        flags = getattr(__import__("subprocess"), "CREATE_NO_WINDOW", 0) | getattr(__import__("subprocess"), "CREATE_NEW_PROCESS_GROUP", 0)
        return {"creationflags": flags}
    return {"start_new_session": True}


def classify_failure(stderr: bytes, exit_code: int | None) -> str:
    sample = stderr[-65536:].lower()
    if b"rate limit" in sample or b"too many requests" in sample or b"http 429" in sample:
        return "provider_rate_limit"
    if b"quota" in sample and any(word in sample for word in (b"wait", b"exceed", b"limit")):
        return "provider_quota_wait"
    if b"provider" in sample or b"api error" in sample:
        return "provider_error"
    return "transport_spawn_error" if exit_code is None else "process_exit_nonzero"
