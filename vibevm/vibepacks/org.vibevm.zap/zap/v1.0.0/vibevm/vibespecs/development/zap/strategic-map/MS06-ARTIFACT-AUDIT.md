# MS06 sealed artifact audit

The fresh ZAP v1.1.0 payload passes the independent source, manifest,
artifact, spec-target, and relative-link checks. **No blockers were found.**

Payload digest:
`a7e268f6a4a2d92f9745935ba30ce7f32962c3e54b756f7f1de87d9b89def992`

- The selected source, 589-row manifest, and sealed artifact independently
  produce exactly **589 files / 7,239,571 bytes** and the same canonical row
  digest. Every source/artifact path, byte length, and SHA-256 matches.
- There are no extra, missing, duplicate, invalid, mismatched, reparse, or
  forbidden entries. `target` segments were checked at every depth together
  with development/private/dependency/token/cache and compiled-output paths.
- The included `specmap.json` has SHA-256
  `e0437f4500ff4a9642d3cc71f3313195156f5ea965e4e88278fb015b21eaeeec`.
  All **687 spec units** resolve to **27 included files**, valid line ranges,
  and present anchors. There are no duplicate URIs, warnings, or suspects.
- **34 published documents / 631,936 bytes** contain **534 links**:
  **480 relative links** to **268 included files** and **54 HTTP(S) links**.
  No relative target is missing or outside the payload. The one fragment,
  `ZAP-RUST-APP-GUIDE.md#internal-boundaries`, resolves.
- External HTTP(S) targets were not network-checked. Historical literal paths
  were not treated as package links. The prior parsed link set for
  `IDEA-MAP.xml` was reused only after its complete artifact SHA-256 was freshly
  verified byte-identical to the independently audited SM05 identity.

[MS06-ARTIFACT-AUDIT.json](MS06-ARTIFACT-AUDIT.json) records the exact roots,
hashes, counts, empty failure sets, and audit method. Artifact reads are
finished. This verifier did not modify the source, payload, manifest, specmap,
installation, build, Git state, or publication state; only these two audit
reports were written. Root retains final acceptance.

