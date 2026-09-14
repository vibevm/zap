# SM05 installed 1.1 strategic-map validation

Status: passed on 2026-09-14 UTC.

## Installed boundary

The two selected tests ran from the Vibe-owned installed slot:

`C:\Users\olegc\.vibe\zap\development\map-1.1-consumer-20260914T1458560928356Z\vibevm\vibedeps\org.vibevm.world.zap\1.1.0`

Both commands used `--release --locked --offline`, the slot-local `target`
directory and an unset `ZAP_R16_NATIVE_PROBE_DIR`. No Cargo, rustc or ZAP test
process overlapped the start of the panel. No ignored, live, host-source or
other package target was invoked.

Installed identities:

- `Cargo.lock`: 15,194 bytes, SHA-256
  `4320a1fcb84b91fb1ef9af97878428a40987e4a8ccb3d6022c926340a2e54f2b`.
- `.vibe-slot.toml`: 74,835 bytes, SHA-256
  `df11146f0eba3441678680ed574c7494c8b4eb5d485cb3c3e5332190e8689792`.

## Results

| Installed target | Result | Executed release binary | Receipt |
| --- | --- | --- | --- |
| `zap-cli --test strategic_map` | 1 passed, 0 failed, 0 ignored | `target/release/deps/strategic_map-b092b836e8f60e6e.exe`, 12,176,384 bytes, SHA-256 `547623fd05909b895e0dc28c6db927b35490fe4dfcefb6a705aeb3b3eee51621` | `SM05-INSTALLED-20260914T151859Z-zap-cli-strategic_map.log`, SHA-256 `e5825adbe964ead67efaf84bf354750fcf8575053a141c1be36a9f1a10b6d50c` |
| `zap-app --test strategic_map_service` | 1 passed, 0 failed, 0 ignored | `target/release/deps/strategic_map_service-43d9f2f4daa1ced3.exe`, 29,911,040 bytes, SHA-256 `561fa3b8d884fc416d688bc76444e9c99a351034ccfd8eb2e52199ccdb5ce54b` | `SM05-INSTALLED-20260914T152005Z-zap-app-strategic_map_service.log`, SHA-256 `3753067b88e39b5c713570776457a77f87692663b40d7b0183fe8ae1f5a0edb8` |

The receipts under
`C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map` record every
installed test source consumed by each target, including the shared seed
module, with its absolute path, byte length and SHA-256. They also record the
Cargo executable, slot-local target, exact command, exit code, executed test
binary and captured stdout/stderr.

The installed compiled-CLI case opens the installed store through the ordinary
generic Query entrypoint and reads overview/object cards. The installed
application case exercises the registered card fingerprint, DataProposal,
direct machine query, authenticated HTTP query, route estimates and stale
cold-reopen path. The two installed cases passed without editing installed
source or the package source. Cargo wrote only derived slot-local target data.

This bounded panel verifies the actual installed 1.1 strategic-map card,
assessment, HTTP and CLI routes. It does not claim another full package panel,
activation, a live model call or a graphical client.
