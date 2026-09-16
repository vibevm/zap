# Specification watch

The specification watch accepts explicit real project roots and bounded include
globs. It ignores generated, dependency, cache, build, and VCS directories,
skips reparse/symbolic links, verifies every resolved path remains below its
configured root, and enforces independent directory-scan, file-count, and byte
limits. Files are hashed through a fixed-size read buffer, so one oversized file
is refused before its contents are allocated.

The digest uses length-prefixed root ordinal/normalized relative paths, exact byte
lengths, and per-file content digests in stable order. Added, changed, and deleted files therefore change the basis while
the same project content remains independent of its absolute checkout path.

Filesystem notifications are debounced advisory signals. `verify` always performs
a fresh bounded capture, so preview/apply catches a change even when a notification
was missed. The watch never parses specification text, grants authority, or
initiates plan application.
