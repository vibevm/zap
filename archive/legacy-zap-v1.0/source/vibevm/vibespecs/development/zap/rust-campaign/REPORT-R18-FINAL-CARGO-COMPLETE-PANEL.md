# R18 final package Cargo complete-panel report

Status: **complete denominator, failed result**. One `--no-fail-fast` invocation ran
every selected unit target, integration target and doctest across all nine ZAP crates.
It found seven failed tests in four targets. This executor reported each failure and
made no Rust or gate-file repair.

## Complete panel

The process removed `ZAP_R16_NATIVE_PROBE_DIR` from its environment and ran:

```powershell
& 'C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1' -CargoArgs @(
  'test', '--workspace',
  '--exclude', 'core-ai-native-specmark',
  '--exclude', 'core-ai-native-specmark-grammar',
  '--no-fail-fast'
)
```

Cargo exited 101 after completing 49 result groups:

| Scope | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Unit and integration tests | 169 | 7 | 16 |
| Doctests | 185 | 0 | 0 |
| **Complete total** | **354** | **7** | **16** |

All 185 doctests passed. This includes 131/131 `zap-wire` doctests, 47/47
`zap-core` doctests, 3/3 `zap-app` doctests, 3/3 `zap-runtime` doctests and the
single `zap-api` doctest.

## Failed targets

Four targets failed:

1. `zap-app --test read_server`
   - `real_loopback_server_snapshot_query_tail_resync_and_cancellation`
   - serde JSON `missing field kind` at line 1, column 378.
   - Read-only diagnosis after the failure found that the fixture's bare store lacks
     the derived index catalog; the viewer query returns a 378-byte `UnsupportedEpoch`
     error which the test then attempts to decode as `MachineResponse`.
2. `zap-domain --test dreamer_service`
   - all four tests failed with `UnsupportedEpoch` under
     `RUST-STORAGE-MANDATORY-INDEXES`;
   - each message says the derived graph index catalog is missing and requires an
     explicit rebuild, with fix surface `Migration`.
3. `zap-domain --test lowering_contracts`
   - `registered_lowering_and_bundle_surface_is_complete` failed at
     `lowering_contracts.rs:27` because an expected family was absent from the
     registered-family collection.
4. `zap-runtime --test runtime_contracts`
   - `runtime_record_registration_is_complete` failed at
     `runtime_contracts.rs:424`: actual registration count 17, expected 16.

All other selected targets passed or contained only the explicitly ignored cases.
The passing legacy targets confirm that the accepted missing-catalog repair from the
first final run remained effective: `legacy_activation` passed 1/1 and
`legacy_import` passed 9/9.

## Ignored and optional-environment limits

The 16 ignored tests were requested by the ordinary workspace command but intentionally
did not run:

- five `zap-app` fresh-process representation-probe phases;
- two native campaign cases requiring a preserved root-owned native probe directory or
  root-owned collaboration receipts;
- three `zap-app` representation-fixture fresh-process phases;
- one controlled R17 knowledge-service performance measurement requiring an explicit
  output directory;
- three `zap-store` fresh-process representation/A2 phases;
- two `zap-wire` fresh-process representation phases.

They are recorded as ignored, not passes. No `--ignored` execution, native probe,
live model, Qwen process or host test panel ran.

The optional material-adapter test
`configured_native_vibe_query_binds_real_spec_bytes_when_fixture_is_available` appears
among the Cargo passes, but `ZAP_R15A_VIBE_EXE` and `ZAP_R15A_VIBE_PROJECT` were
unset. This is not actual Vibe integration evidence.

## Machine evidence

Run ID: `R18-FINAL-COMPLETE-20260914T120410Z`.

- Log:
  `C:/Users/olegc/.vibe/zap/development/vibevm-next/final-validation/R18-FINAL-COMPLETE-20260914T120410Z-test.log`,
  47,745 bytes,
  SHA-256 `aae6970b0c32a70d49654497f6a18f4537c6cbddc1a7f57647628d82adbf6864`.
- Log lifetime: 156.378 seconds, from `2026-09-14T12:05:20.6872617Z` through
  `2026-09-14T12:07:57.0653217Z`.

No clippy or formatting gate was repeated. The earlier all-nine strict all-target
clippy and formatting results, plus the changed-app strict result, remain separate
evidence and do not override these four behavioral target failures.

No Git operation, publication, source repair or shared index/conformance write ran in
this executor task. The package test gate remains failed pending finite repair and
focused verification of the four named targets.
