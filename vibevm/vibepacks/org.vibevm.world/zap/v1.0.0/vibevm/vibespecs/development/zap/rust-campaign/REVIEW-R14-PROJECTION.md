# Root review of the R14 normalized import candidate

Status: changes requested during implementation; no R14 full acceptance yet.
Reviewed current app legacy-import service/translation and domain projection
sources on 2026-09-13. The earlier archive-preparation findings remain in
REVIEW-R14-PREPARATION.md. This note records concrete current boundaries rather
than treating preparation or passing fixture counts as complete migration.

The candidate now materially derives Work, inactive TaskContract, Obligation,
Mandate and retained node/task-constraint records from independently validated
source bytes. Its official cell recomputes the submitted projection and ID map.
R08's accepted contract supplies later explicit inactive-contract activation or
replacement; legacy command strings are not executable argv.

Required repairs before acceptance:

1. Translation reads genesis base. Supported nonzero legacy replay may contain
   classifications, evidence, facts and regions. Either preserve all such
   current semantics as queryable untrusted draft data or refuse normalized
   import explicitly. Apply the same rule in both the app preparation and the
   official import cell/replay path. A caller with a valid service handle must
   not bypass a high-level-only refusal and silently reset to genesis.
2. `projection_counts` scans one page limited to 4096 and requires complete
   output. Paginate instead of imposing an implicit total plan-size ceiling.
3. `recover_staged` currently requires an already-complete receipt. Recover a
   crash after database commit but before receipt creation by reconstructing
   and verifying the exact requested command/source, reconciling the database,
   then safely publishing the receipt. Cover precommit staging as well. Never
   overwrite or remove an unrelated destination/staging directory.
4. Published/staged verification must compare actual StoreIdentity and the
   committed import's exact source manifest/command to the requested identity
   and independently validated source, not only compare receipt labels with
   projection digest/counts.

Keep source hashes unchanged, output isolated and inactive, and preserve exact
legacy constraint/raw evidence. The authorized actual NEXT input is revision 0
with 425 nodes, 212 contracts, 64 mandates and 1292 obligations. A successful
actual import must include query/reopen/audit evidence and the no-activation
boundary; it does not start any NEXT task.
