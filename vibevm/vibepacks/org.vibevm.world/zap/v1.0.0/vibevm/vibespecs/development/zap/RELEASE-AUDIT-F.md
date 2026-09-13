# ZAP public release and status audit

## Audit boundary

This began as a read-only publication review of the package's XML artifacts,
README, three skills, two public API documents, six examples, manifest and
ignore rules. It proposes corrections and status transitions; it does not
accept clauses, edit permanent status, publish, install, activate a charter or
run NEXT.

## Remediation result

Root subsequently authorized every concrete documentation/usability repair in
this report. The permanent wording corrections are applied without changing
any acceptance status. `ZAP-PYTHON-API.md` now ships the stable embedding
surface; capabilities expose the runtime profile JSON Schema and exact nested
knowledge event JSON Schemas with explicit dialect/version; the examples guide
labels state-bound files honestly and links the runnable isolated live probe;
the sparse template, pre-activation skill description and custom-argv wording
are present. The status-promotion proposal below remains pending root evidence.

## Release-blocking wording corrections

The following permanent text was stale or created an authorization ceremony
the owner did not request. These corrections are now applied before status
promotion:

1. `ZAP-ADAPTIVE-CYCLE.xml` top comment says final completeness is accepted by
   the Owner after an integration review. `ZAP-DATA-AND-VIEWER.xml` and
   `ZAP-METHODOLOGY.xml` carry the same owner-gate wording. Replace all three
   with: **“Integration completeness is confirmed by the coordinator from the
   complete denominator and recorded evidence; no additional owner approval
   ceremony is required.”** Keep owner authority for charter amendments,
   reserved actions and pauses; this correction concerns release acceptance,
   not campaign authority.
2. `ZAP-ADAPTIVE-CYCLE.xml` `ATOMIC-REVIEW` began “Future transition”. The
   transition, sparse expansion, CAS and proposed/applied split now exist.
   Replace “Future transition” with “Adaptive transition”.
3. `ZAP-DATA-AND-VIEWER.xml` section `adaptive-review` was titled “Future
   adaptive-review contract”. Remove “Future”. The future canvas is
   still future work; the review contract is current.
4. In the same file, `DATA-STATUS` calls `zap/1` a prototype container and
   `PROTOTYPE-FILES` calls the current store a prototype three times. Preserve
   the stable element IDs to avoid breaking anchors, but change prose to
   “current `zap/1` container/store format”. Its final sentence should say the
   local store contract is separate from the implemented HTTP contract, rather
   than implying that no network protocol exists.
5. `PLANNED-EVENT-FAMILIES` says its listed plan/decision/invalidation/rule/
   pause/wait/result/acceptance/promotion families are not asserted as
   supported today. All are now present in the composed registry. Change this
   to say the registry and capabilities are authoritative and future viewer
   scenarios consume those supported families.
6. README calls `command-contract.json` the contract of the “implemented
   prototype”. Replace that one phrase with “implemented protocol”. Other uses
   of “prototype” describe the methodology's maturity axis and are correct.

Two non-normative research sentences need historical qualification rather than
silent rewriting of their evidence:

- `IDEA-MAP.xml` ZAP-I19 and its byte-matched `idea-map.json` say the cycle “is
  designed, not implemented by the current additive kernel”. Change “current”
  to “the additive kernel available when this research snapshot was authored”.
- `PILOT-REPORT.xml` `PILOT-LIMITS` says trusted control, native-fact
  reevaluation, runner and server stream are not implemented. Prefix that list
  with “At the time of this 2026-09-12 pilot”. Keep its 18-test count, 12-event
  count, kernel hash and `doc/done` status: they are historical experiment
  evidence, not the current release denominator.

## Public usability and documentation gaps

These gaps did not justify inventing another event or authority route. They are
now addressed through public documentation, capabilities, templates and the
existing live-probe module:

1. `.vibeignore` excludes all `vibevm/vibespecs/development/**`. The shipped
   package therefore omits the only exact Python APIs for foundation/storage,
   control/service/trust, domain, knowledge, runner and transport. CLI and HTTP
   users have public documents, but an embedding host must inspect source or
   instantiate a store and query capabilities. Publish one consolidated
   `flows/zap/ZAP-PYTHON-API.md`, or reviewed public copies of
   `FOUNDATION-API`, `STORAGE-API`, `CONTROL-API`, `DOMAIN-API`,
   `KNOWLEDGE-API`, `RUNNER-API` and `TRANSPORT-API`. Include only stable public
   exports, callback signatures, route/role boundaries, sparse-review builder,
   artifact-capture ordering and the no-distributed-transaction limit.
2. The public CLI document describes runtime profiles, but no shipped
   machine-readable profile schema describes `worker.kind`, PATH/explicit
   launcher behavior, `required_inherited_environment`, `artifact_capture`,
   transport stop mode and verification bindings together. Add that descriptor
   to capabilities/command-contract or a public `runtime-profile.schema.json`.
3. `backend-config.json` is not consumed by `zap.py serve`; the real interface
   is CLI flags. Remove the file, rename/label it as an illustrative flag
   projection, or add a documented loader in a separately authorized change.
   Do not imply that passing this JSON configures the server today.
4. `action.json` combines action and assessment fragments but lacks the exact
   product command required by `zap.py action`, so it is not runnable. Split it
   into the three input files or label it explicitly as a non-runnable schema
   fragment.
5. `charter.json` has an all-zero intent fingerprint and empty
   `legacy_authority`; it will correctly fail for an imported plan with
   mandates. Add a paired intent proposal/fingerprint example and state that
   every imported mandate requires a real classification. A small public
   fingerprint preparation command/example would remove the remaining manual
   hash step.
6. Add `examples/zap/sparse-review-transition.json` using the frozen 13-field
   request. The helper is public in CLI/backend/capabilities, but its only exact
   field list currently lives in excluded development material or runtime
   capabilities.
7. `zap-run`'s frontmatter says it applies only after explicit activation, but
   the skill itself is also the documented route for import, trust bootstrap
   and activation. Change the description to “Configure, activate and operate
   ...”; keep the body prohibition on implicit activation.
8. `ZAP-CLI.md` says worker/verification argv always contains
   `{packet_file}` immediately before explaining that ready `codex-sol-xhigh`
   input uses `argv: []` and constructs the bridge. Limit the first sentence to
   custom argv profiles. The later ready-profile explanation is accurate.

## Shipping inventory

Applying the current `.vibeignore` rules to the candidate tree yields 132
shipped files: 102 Python, 10 JSON, 9 XML, 9 Markdown, `vibe.toml` and
`.vibeignore`. The public topology is:

- one boot snippet;
- five normative flow XML files (including the design-only change-economics
  phase) plus three public Markdown API files;
- three non-normative research XML files and three paired JSON inventories;
- seven JSON examples plus their public guide;
- three skill entrypoints, 61 `zaplib` modules, 38 test/support modules and
  three executable/compatibility wrappers.

The manifest includes `zap-draft`, `zap-state` and `zap-run`, so the public
Python entrypoint and runtime locator are selected into package/skill
projection. Public flow/API/research/example files are not ignored. The ignore
rules correctly remove development reports/APIs, bytecode, token files,
`trust.json` and `private/**`.

Tests are currently shipped because no ignore rule excludes them. Keeping them
provides an offline package self-check and includes no observed credential or
private fixture bytes; excluding them would reduce payload size but would also
remove that self-check. Make this an explicit release choice rather than an
accidental side effect.

All shipped Markdown relative links resolve in the candidate tree. All eight
XML files parse, and the six JSON examples parse. The `zap-run` skill passes the
skill-creator validator. This audit did not run package publication or install.

## Exact status-promotion proposal

No status below should move until root's final combined tests, live runtime
proof where named, package validation, publication and isolated install supply
the referenced evidence. Coordinator acceptance from that evidence is the
gate; there is no extra owner approval ceremony.

1. **Normative method:** after root's complete-spec review, promote
   `ZAP-METHODOLOGY.xml` top status from `spec/plan` to `spec/done` and its 49
   requirements from `spec/plan` to `spec/done`. This records a completed
   normative method without claiming that every qualitative judgment is a
   machine-enforced reducer. Promote the boot snippet's `SELECTION`,
   `SCOPED-READING` and `CANON` clauses and its top status to `spec/done` at
   the same boundary. Hold boot `CAPABILITIES` for the runtime acceptance below.
2. **Adaptive semantics:** after the final B/B2/domain integration gate,
   promote the file's top status to `spec/done` and its remaining semantic/case
   clauses to `spec/done`. Promote only `LOSSLESS-REVISION`, `PLAN-RECONCILIATION`,
   `LIVE-WORK-RECONCILIATION`, `EVIDENCE-APPLICABILITY`, `REVISED-DEFERRALS`,
   `REVIEW-RECORD`, `ATOMIC-REVIEW` and `PROTOTYPE-BOUNDARY` to `impl/done`,
   because those have direct event/reducer/runtime evidence. Keep the worked
   examples as `spec/done`, not implementation claims.
3. **Runtime:** promote each of the 11 `ZAP-RUNTIME.xml` clauses individually
   to `impl/done` only when its evidence exists. Current focused tests support
   `PRESERVE-AND-EXTEND`, `MODULE-BOUNDARIES`, `TRUSTED-CONTROL`,
   `CONTROL-BINDING`, `ADAPTIVE-DOMAIN`, `RECOVERY-AND-RESOURCES`,
   `FACTS-AND-PROOF` and `BACKEND-CONSISTENCY`. Hold `ACTUAL-RUNNER` for the
   final real Sol/xhigh proof; hold `PORTABLE-RUNTIME` for projected-skill
   install proof; hold `MIGRATION-AND-RELEASE` for final 425/212/64/1,292
   migration evidence plus package validation, publication and isolated install.
   Promote the file's top status to `impl/done` only after all 11 are done.
   At that same boundary promote boot `CAPABILITIES` to `impl/done`.
4. **Data/backend:** after the final combined gate, promote only these directly
   demonstrated clauses in `ZAP-DATA-AND-VIEWER.xml` to `impl/done`:
   `SUPPORTED-SUBSET`, `PROTOTYPE-OPERATIONS`, `IMMUTABLE-BASE`,
   `PROTOTYPE-FILES`, `LOSSLESS-IMPORT`, `PILOT-DENOMINATOR`,
   `IMPORT-AUTHORITY`, all seven clauses in section `graph`,
   `ADAPTIVE-REVIEW-INPUTS`, `ADAPTIVE-REVIEW-ASSESSMENT`,
   `ADAPTIVE-REVIEW-TRANSITION`, `ADAPTIVE-WIRE-STATUS`, all eight clauses in
   section `events`, all six clauses in section `authority`,
   `LOGICAL-AND-PHYSICAL-STATE`, `RESOURCE-WAIT`, `CANVAS-DATA-ACCESS`,
   `SNAPSHOT-AND-CURSOR`, `STREAM-RECOVERY` and `PLANNED-EVENT-FAMILIES`.
   Promote `DATA-STATUS`, `STORAGE-OWNERSHIP`, `PROJECT-LOCAL-SPLIT` and
   `DATA-METHOD-REFERENCE` only to `spec/done` after the complete-spec review.
   Keep `CANVAS-EXPERIENCE`, `LARGE-GRAPH-NAVIGATION`,
   `IN-CANVAS-NODE-DETAIL`, `CLICKABLE-ENTITY-DETAIL`,
   `STRATEGY-MAP-METAPHOR`, `HEROES-ART-DIRECTION`,
   `ORTHOGONAL-VISUAL-CHANNELS`, `KNOWLEDGE-VISUAL-DISTINCTIONS`,
   `ADAPTIVE-VIEW`, `VIEWER-SCOPE`, `VIEWER-EXPLANATIONS`,
   `READER-PREREQUISITES`, `MACHINE-ACTIONS` and `VIEWER-ACCEPTANCE` at
   `spec/plan`: the backend supplies their data, while the interactive canvas
   behavior and visual acceptance remain future work. The mixed file's top
   status therefore stays `spec/plan`.
5. **Research/pilot:** keep `ANCILLARY-REVIEW.xml`, `IDEA-MAP.xml` and
   `PILOT-REPORT.xml` at `doc/done`. Their status describes completed historical
   artifacts and must not be repurposed as current implementation acceptance.
6. **Machine contract:** after root accepts the actual registry/CLI/backend
   denominator, change `command-contract.json.status` from
   `implementation_candidate` to `implemented`. This is an interface status,
   not authorization for any campaign.

If any held runtime proof or public-doc gap remains at publication time, leave
its status pending and name that specific limitation in README. Do not use a
successful publish operation itself as evidence for adaptive correctness,
control authority, safe stopping or canvas behavior.
