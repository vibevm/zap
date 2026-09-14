# MS06 final Cargo gates

Status: **green** for the five changed crates. Commands used the required shared-target wrapper from the 1.1.0 package root. No live probe, performance pilot, package build/install, or Git command was run.

## Results

- Five-crate test boundary: **355 passed, 0 failed, 13 ignored** across 43 Cargo test summaries. The first parallel compilation attempt exhausted rustc memory before completion; the exact Cargo argument boundary was rerun with `CARGO_BUILD_JOBS=1` and passed.
- Final regrouped domain targets: **36 passed, 0 failed, 1 ignored**. This covers the final move of milestone scenarios into `lowering_service` and `knowledge_service` after the 355-pass baseline. Production code did not change for that move, so the full baseline plus these final target checks are the accepted relationship.
- Strict five-crate all-target clippy: **0 warnings, 0 errors** under `-D warnings`. Two failed attempts are retained: the first exposed an oversized strategic-map fixture variant; the second exposed dead cross-target milestone fixture APIs. Both were repaired without lint allowances or fake uses, and the exact boundary passed on attempt three.
- Formatting: `cargo fmt --all --check` passed with no differences.

## Logs

- Test success: `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T171843Z-test-attempt2-jobs1.log`
- Test allocator-failure evidence: `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T171721Z-test.log`
- Regrouped tests: `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T173131Z-regrouped-domain-tests.log`
- Clippy success: `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T173018Z-clippy-attempt3.log`
- Clippy failure evidence: `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T171721Z-clippy.log`; `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T172358Z-clippy-attempt2.log`
- Formatting: `C:\Users\olegc\.vibe\zap\development\vibevm-next\strategic-map\MS06-FINAL-20260914T171721Z-fmt.log`

Machine receipt: `vibevm/vibespecs/development/zap/strategic-map/MS06-FINAL-20260914T171721Z-CARGO-RECEIPT.json`.

