# Final sealed publication artifact audit

**The final 507-file artifact passes the independent file and local-link
audit. No blocking findings remain in this audit.**

Manifest:
`C:/Users/olegc/.vibe/zap/development/zap-publication-20260914T1234104656056Z/R15B-PAYLOAD-MANIFEST.json`

Payload digest:
`d426892fc6d5980061e5b06aa5576bc6fe39bc9325811707183c360ed0029fe5`

Verified at 2026-09-14 12:36:18 UTC:

- Exactly **507 actual files**, totalling **6,243,786 bytes**, match every
  manifest path, length and SHA-256; the recomputed digest matches.
- No missing, extra, mismatched, reparse or forbidden-category paths occur.
  A `target` directory segment is checked at every depth.
- All **28 public document files** match the previously audited document bytes.
  Their **456 relative links** resolve to **259 included, existing files**;
  the one fragment also resolves. The 54 HTTP(S) links were not network-checked.
- Relative to the preceding 507-row manifest, only `discipline/DEBT.md` and
  `discipline/registry/debt.json` differ. No Rust, Cargo or public-document
  change was observed.

The rejected 525-file cache payload and the later 509-file scratch container
remain recorded in their separate audits. Their rejection is not overwritten
by this successful check of a different sealed directory.

Details and document identities are in
[the JSON receipt](R15B-PUBLICATION-ARTIFACT-AUDIT.json). Root's installed
build/tests and release acceptance remain separate evidence. This verifier
made no artifact/source changes, ran no tests and performed no publication.

