# R15B final installed-source tests

Status: passed on 2026-09-14 UTC.

## Installed boundary

The tests ran from the Vibe-owned installed slot:

`C:\Users\olegc\.vibe\zap\development\r15b-final-consumer-20260914T1132402642961Z\vibevm\vibedeps\org.vibevm.world.zap\1.0.0`

Cargo was `C:\Users\olegc\.cargo\bin\cargo.exe`. Every invocation used the
slot-local `target` directory, `--release --locked --offline`, and an unset
`ZAP_R16_NATIVE_PROBE_DIR`. No ignored, live, or host-source test was invoked.
The only observed `vibe.exe` process was the long-running `vibe mcp serve`
process; no Cargo, rustc, ZAP, packaging, or slot-writing process overlapped the
start of this panel.

Installed identities:

- `Cargo.lock`: 15,194 bytes, SHA-256
  `936d1cb6355a054e05129fe26c974d62df26a8f754f74dfbe4ac62f3609453d8`.
- `.vibe-slot.toml`: 70,840 bytes, SHA-256
  `4f3a68d218d593d4beb5aa458383d42377da69e43106879f4179feef9b0b89a5`.

## Results

| Installed target | Result | Installed source SHA-256 | Executed release binary SHA-256 | Receipt |
| --- | --- | --- | --- | --- |
| `zap-cli --test binary_server` | 2 passed, 0 failed, 0 ignored | `37074c109d8b2cc0702f3e3246109e4d991f1bf235fdae3368c68a51d685d661` | `db9168aa6a1340a37631917b18931a4e76894896ff6d7d2abd6904681aca151b` | `R15B-INSTALLED-20260914T122806Z-binary_server.log` |
| `zap-app --test application_server` | 4 passed, 0 failed, 0 ignored | `395f8b00cf13afe2d19aebe63543450230ed068b115e26e28f908be0ce45b3dc` | `903a5f23b034389795fd96873272794ad3639c3b6ae1e6e71a13de03d281c60b` | `R15B-INSTALLED-20260914T122913Z-application_server.log` |
| `zap-app --test packet_resolution_service` | 2 passed, 0 failed, 0 ignored | `15d91f664ad6b6bf664ab7ab27d082f5044078ee5ab564661c3d2def55946400` | `e91d8b5853c801bab681c7ef04a5a54dc5213c4291983dd66b0c11fdc7d7da2a` | `R15B-INSTALLED-20260914T122946Z-packet_resolution_service.log` |

The eight selected installed-source cases passed. Each receipt records the
resolved installed source path, source hash, Cargo and target paths, command,
exit code, executed release test-binary path and hash, and captured standard
output/error. The receipts are under
`C:\Users\olegc\.vibe\zap\development\vibevm-next\final-validation`.

The three installed test-source hashes were unchanged after execution. Cargo
wrote only the installed slot's derived `target` directory; installed source
was not modified.

## Receipt note

`R15B-INSTALLED-20260914T122709Z-binary_server.log` is explicitly marked
`receipt_status=invalid_capture_wrapper_error`. PowerShell rejected the first
capture wrapper before Cargo started, no test binary was produced, and the
attempt is not counted. The succeeding unique receipt above is the executable
test evidence.

This panel proves the selected binaries and application paths from the actual
installed package source. It does not substitute for a live-harness test or a
second full-package panel.
