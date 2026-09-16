# Root review: R17 admission locality

Accepted as a bounded implementation and evidence unit on 2026-09-14. This is
not final source-artifact acceptance or publication. The raw record-page repair
and final stable-source gates remain separate work.

Root inspected the final affected-scope traversal, admission index contributions
and readers, pause/exception selection, runtime affected-job provider, and the
actual whole-transaction scan-rejecting store and WorkRenamed fixture. The
previous review corrections for last-active-contract selection and absent Work
or Obligation records are retained. Contract history contributes in canonical
record order; directed knowledge expansion does not feed back into initial
contract-consumer selection. The copied prior implementation supplies the
differential oracle, including cycles, inactive records, incident edges and
incomplete closure boundaries.

The WorkRenamed test uses the production registered cell and CommitService.
Its store wrapper rejects both SnapshotRead::scan and StateReader::scan_erased
through the entire transaction, delegates exact/index reads and commit binding,
and asserts an empty scan map. Observed provider calls are admission 4,
affected scope 1, and affected jobs 1. The registered basis provider is correctly
unused because this command carries no payload basis request. The separate
Progress/Proof fixture is evidence for shared providers only.

Root also inspected the real-store active-job test: 4,101 unrelated terminal
observations precede the selected active and terminal-with-Unknown observations;
replacement removes the completed contribution, reopening retains the result,
and a stale revision refuses evaluation. The production provider threads one
65,536-row budget through every requested work and subject partition. Its small
unit test verifies cumulative budget arithmetic; it does not itself execute
overlapping index partitions. Source inspection establishes that integration.

Admission budgets have narrower scopes: affected-scope derivation shares its
budget across closure reads; pause and exception lookup share another budget;
live-hold enumeration has its own budget. Repeated admission lifecycle phases
can repeat these reads. No whole-command 65,536-row limit is claimed.

The focused execution receipts are recorded in REPORT-R17-ADMISSION.md and
checkpoints/R17-ADMISSION.json: selected service tests, differential scope and
catalog-refusal cases, active-job reopen case, strict library/test lint, domain
formatting, and four scoped conformance checks passed. The later R18-G structure
changes retain their own preservation evidence and gates. A second identical
test run is unnecessary before the queued raw-page seam changes; final package
verification will use the coherent resulting source.

Initial construction, semantic economics lookup, campaign-global completion,
project-wide source operations, and live-hold enumeration are outside the
measured ordinary-command locality result. No data/command/event epoch, authority
policy, serialized public-query format, Git history, or external publication
was changed by this acceptance.
