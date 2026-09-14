# MS06 installed consumer proof

Status: **installed build and bounded installed tests passed**.

## Installed boundary

The proof used the fresh consumer at
`C:\Users\olegc\.vibe\zap\development\milestone-1.1-consumer-20260914T1653575271008Z`
and installed slot
`vibevm\vibedeps\org.vibevm.world.zap\1.1.0`.
The source registry was the matching timestamped registry. No source, sealed payload, or installed source file was edited.

The source manifest contains **589 files / 7,239,571 bytes** with payload digest
`a7e268f6a4a2d92f9745935ba30ce7f32962c3e54b756f7f1de87d9b89def992`.
Every payload file was hashed. The installed slot matched all 588 materialized payload files exactly; source `.vibeignore` was intentionally omitted and `.vibe-slot.toml` was the sole installed extra. There were zero path or hash mismatches. Before the build, the installed slot had no `target/`.

## Cold installed build

After the independent artifact audit released the registry, both absolute rename paths were checked under the user-local development root. The complete timestamped registry was moved to a fresh unavailable sibling. With `CARGO_TARGET_DIR` unset and `CARGO_BUILD_JOBS=1`, the consumer ran:

`vibe bin build zap --assume-yes --offline --unattended --invoked-by codex --agent-mode agent`

The release build completed in **150,256 ms** while the registry was unavailable. The `finally` path restored the registry and verified the unavailable sibling no longer existed. `vibe bin path` resolved `zap` to the installed slot's `target\release\zap.exe`: **30,563,328 bytes**, SHA-256 `323a0f7529088e0beb3ad2a3e648f9983275b86e832a75965b6e8d53d0265304`.

The bare `zap capabilities` invocation exited successfully and is retained as the defined default process-availability probe. Its empty `query_ids` is not a configured-store registry claim. Actual query registration was verified through installed store-scoped CLI and configured application-service paths.

## Installed release tests

Both tests used the installed source, `--release --locked --offline`, `CARGO_BUILD_JOBS=1`, an installed-slot-local `CARGO_TARGET_DIR`, and an unset `ZAP_R16_NATIVE_PROBE_DIR`.

- `zap-cli --test strategic_map`: **1 passed, 0 failed, 0 ignored**, 48,152 ms. It exercised store-scoped MachineRequest queries for `zap.milestone.read`, `zap.milestone.plan`, `zap.information.opportunities.v1`, `zap.map.overview.v1`, and `zap.map.object.v1`.
- `zap-app --test strategic_map_service`: **2 passed, 0 failed, 0 ignored**, 49,931 ms. It verified configured capabilities for the milestone/information queries, protected information commands, direct machine queries, authenticated HTTP, canonical milestone/information cards, route estimates, and cold reopen.

The machine receipt records Cargo.lock, slot metadata, all selected test/shared source hashes, executed release test binary hashes, build/test durations, query identities, log hashes, parity, and final registry restoration: `MS06-INSTALLED-RECEIPT.json`.

## Evidence logs

- `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-INSTALLED-20260914T173829Z-bin-build.log` — SHA-256 `5aa1d6759cf5ed40a8d14ffaf4346416d0667a4824e3f1a4a5f50655162f2dc3`
- `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-INSTALLED-20260914T173829Z-capabilities.log` — SHA-256 `c05cae15f01d90ca88e878a53d475702f71f5d01ca95b2487d68c841313beed3`
- `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-INSTALLED-20260914T173829Z-cli-strategic-map.log` — SHA-256 `debf3f98ad91c4136523ace0d18cc0d02c45bee4528360501e479d2c3ddcbd4d`
- `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-INSTALLED-20260914T173829Z-app-strategic-map-service.log` — SHA-256 `69b4a8df56c34df5c10f4eb7506588f57efe0b4ac999bd1cd65a111b2f33a140`

No publication, Git operation, live model, network fetch, performance pilot, or shared host target was used.

