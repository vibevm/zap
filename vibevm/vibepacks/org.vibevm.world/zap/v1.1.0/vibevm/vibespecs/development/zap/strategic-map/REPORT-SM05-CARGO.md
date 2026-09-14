# SM05 final changed-package Cargo regression

Status: **passed**. The coherent frozen 1.1.0 source passed the complete `zap-domain`
unit, integration, and doctest selection, strict all-target clippy for the three changed
integration crates, and formatting for those crates. This executor changed no Rust or
public guide source.

## Domain regression

The fixed Cargo runner executed:

```powershell
& 'C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1' -CargoArgs @(
  'test', '-p', 'zap-domain', '--offline', '--no-fail-fast'
)
```

Cargo exited 0 after all 15 result groups:

| Target | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| `zap-domain` library | 7 | 0 | 0 |
| `domain_contracts` | 8 | 0 | 0 |
| `dreamer_service` | 4 | 0 | 0 |
| `economics_contracts` | 5 | 0 | 0 |
| `economics_service` | 12 | 0 | 0 |
| `knowledge_contracts` | 7 | 0 | 0 |
| `knowledge_service` | 17 | 0 | 1 |
| `lowering_contracts` | 5 | 0 | 0 |
| `lowering_offline` | 1 | 0 | 0 |
| `lowering_semantic_service` | 1 | 0 | 0 |
| `lowering_service` | 1 | 0 | 0 |
| `map_assessment` | 4 | 0 | 0 |
| `r07_review_repair` | 2 | 0 | 0 |
| `strategic_map` | 5 | 0 | 0 |
| `zap-domain` doctests | 0 | 0 | 0 |
| **Total** | **79** | **0** | **1** |

The single ignored case is
`r17_performance::measured_registered_queries_on_mixed_graph`, an existing controlled
performance measurement that requires an explicit output directory. It was not counted
as a pass and no `--ignored` execution was requested.

`ZAP_R16_NATIVE_PROBE_DIR` was unset. The lowering fixtures used their deterministic
in-process source where selected; this run supplies no external native-probe or live
collaboration evidence. No other conditional environment test was skipped in the
selected domain denominator.

## Strict lint and formatting

Strict all-target clippy ran for the three changed integration crates:

```powershell
& 'C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1' -CargoArgs @(
  'clippy',
  '-p', 'zap-domain', '-p', 'zap-app', '-p', 'zap-cli',
  '--all-targets', '--offline', '--', '-D', 'warnings'
)
```

It exited 0 after checking all three crates and their selected targets.

Formatting ran as:

```powershell
& 'C:/Users/olegc/.vibe/zap/development/vibevm-next/run-cargo.ps1' -CargoArgs @(
  'fmt',
  '-p', 'zap-domain', '-p', 'zap-app', '-p', 'zap-cli',
  '--', '--check'
)
```

It exited 0 with no output.

## Machine evidence

Run ID: `SM05-FINAL-20260914T150405Z`.

- Domain test log:
  `C:/Users/olegc/.vibe/zap/development/vibevm-next/strategic-map/SM05-FINAL-20260914T150405Z-domain-test.log`,
  10,663 bytes,
  SHA-256 `8a92749d7743d567a2688dfc05f5f5f17627bd8dd38d2c26e8503146268c0a9d`.
- Clippy log:
  `C:/Users/olegc/.vibe/zap/development/vibevm-next/strategic-map/SM05-FINAL-20260914T150405Z-clippy.log`,
  452 bytes,
  SHA-256 `7465332233e5750679bba632a8833f02f21bbe423e7330b1def9a489fc979e0f`.
- Formatting log: the successful command produced no stdout, represented by
  `C:/Users/olegc/.vibe/zap/development/vibevm-next/strategic-map/SM05-FINAL-20260914T150405Z-fmt.log`,
  0 bytes,
  SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`.

The test log spans `2026-09-14T15:04:06.2724884Z` through
`2026-09-14T15:05:02.3609121Z`. The clippy log spans
`2026-09-14T15:05:18.5401141Z` through `2026-09-14T15:05:35.9951858Z`.

No unchanged core/runtime/full-workspace regression, ignored test, live inference, host
panel, specmap generation, assembly, installation, publication, Git operation, or silent
source repair ran in this executor task.
