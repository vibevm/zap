# SM05 sealed artifact audit

The v1.1.0 artifact passes the independent file, spec-target and relative-link
checks. **No blockers were found.**

Payload digest:
`20174b25ada907db49852ba59f4d75b256b9d5e1c9892a7dd85e2551633a3144`

- Exactly **535 files / 6,560,722 bytes** match the complete manifest path set,
  lengths and SHA-256. The recomputed digest matches.
- No extra, missing, mismatched, reparse or forbidden paths occur; `target`
  segments were checked at every depth.
- All **643 specmap spec units** point to **23 files** inside the artifact.
- **30 published documents** contain **468 relative links** to
  **264 included files**. The one fragment resolves. Twenty-two unchanged
  documents were reused by exact byte hash; eight were read afresh.
- The 54 HTTP(S) hyperlinks were not network-checked. Historical literal paths
  were not treated as package links.

[SM05-ARTIFACT-AUDIT.json](SM05-ARTIFACT-AUDIT.json) records the manifest identity,
checks and document hashes. Artifact reads are finished; root may keep the
registry hidden during its separate installed build. No artifact/source writes,
tests, specmap, Git or publication operations were performed by this verifier.
Root retains final acceptance and publication.

