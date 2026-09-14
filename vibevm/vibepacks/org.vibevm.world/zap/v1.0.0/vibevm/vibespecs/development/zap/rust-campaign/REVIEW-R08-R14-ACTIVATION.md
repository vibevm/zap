# Root acceptance of the real imported-draft activation boundary

Root read REPORT-R08-R14-ACTIVATION.md and the current integration test. The
test imports actual tiny zap/1 bytes through the public importer, reopens that
same store, uses real registered lifecycle/strategy/lowering commands, and
checks inactive contract v1 -> active contract v2 without changing the imported
Work identity/value, legacy metadata, task constraints or original source bytes.
It drops the service and verifies the result after reopening.

The focused1/1, warning-denied clippy and exact formatting receipts support this
boundary. No additional suite was run solely for root review. This closes the
named R08/R14 activation evidence gap; it does not activate actual NEXT.

Together with REPORT-R14-FINAL.md and REVIEW-R14-RECOVERY.md, the actual
revision-zero425/212/64/1292 inactive migration, exact source preservation and
bounded recovery are accepted. Normalized nonzero legacy semantics still refuse
explicitly, as documented; they must not be advertised as supported. Separate
R17 representation/scale gates remain open and prevent full-product completion.
