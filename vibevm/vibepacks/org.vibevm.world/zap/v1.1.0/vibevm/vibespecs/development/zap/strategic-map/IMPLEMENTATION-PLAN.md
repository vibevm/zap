# Strategic map implementation, 1.1.0

Owner authorization: the latest conversation explicitly approves beginning
implementation of the strategic-map review and the subsequent human-readable
card/complexity/difficulty discussion. This supersedes the old discussion-only
instruction for this development. It does not authorize NEXT execution or a
graphical client. The released1.0.0 source and tag remain unchanged.

The working slot is zap/v1.1.0 in C:/Users/olegc/git/v/vibevm-next, branch next.
The baseline is the507-file published1.0.0 source at host commit85d52812,
followed by the accepted review/evidence commit95fa9de6. The approved input is
preserved as APPROVED-DESIGN-INPUT.md. The implementation contract is the new
flows/zap/ZAP-STRATEGIC-MAP.xml in this slot.

## Execution boundaries

Root owns architecture, acceptance, plan/checkpoints, final integration review
and commits. Native Sol/xhigh workers implement code. The inherited non-code
helper prepares only read-only pilot data. No external GLM/Qwen launcher or
live-model experiment is selected. All Vibe installs use --offline.

Ordinary Cargo work uses the existing user-local run-cargo.ps1 wrapper and its
fixed C:/Users/olegc/.vibe/zap/build/next-rust target. The standalone
rust-ai-native tool already built there checks this slot by --path. A final
installed-consumer proof can use its own Vibe-owned target as in R15B; no host
source path can become a package dependency. Do not run the full host panel.

## Work and ownership

| Atom | Owner | Boundary and acceptance |
| --- | --- | --- |
| SM01 | Root | New clean version slot, exact contract and durable plan. Root Cargo/vibe versions are1.1.0; locked offline all-target check passed. |
| SM02 | zap_floor | Public strategic_map module, exact selected-strategy overview, bounded cards/typed relations, conservative route estimates, query registration and real usage documentation. |
| SM03 | zap_locality | Separate map_assessment data record/proposal, exact Work/active-contract source fingerprint, descriptive-only CAS/retry, source freshness and validated independent estimate/grade fields. |
| SM04 | Root, then available coding worker | Actual generic Query/HTTP/CLI journey and read-only21-task development pilot with49planning plus9acceptance edges; no canonical migration. |
| SM05 | Root and workers by changed boundary | Source review, relevant regression/conformance/public-use/specmap gates, package documentation and clean new-slot install/build proof. |

Workers may edit their named new modules/tests/guides. Coordinate small lib.rs
and registration.rs additions; do not overwrite the other owner's composition.
Changes to existing source are limited to explicitly reviewed reuse seams, such
as the complete active-contract selection helper. Keep source files at or below
600lines and bind each new public type to real usage documentation or a useful
compiled example. No gate exemption, frozen baseline widening or inert wrapper
substitute is allowed.

## Integration decisions

Generic QuerySpec/QuerySet already reaches the machine API, authenticated HTTP
query handler and CLI. The runtime CampaignReadPort/frontier is a different
boundary and is not replaced for a visualization feature.

Overview selects exact StrategicRevisionId; there is no guessed global current
strategy. An unmaterialized strategic node can still be described, with missing
live Work state explicit. Milestone/landmark is a facet of existing structural
work kinds, not a new canonical acceptance authority.

The shared assessment interface includes MapWorkAssessmentRecord keyed WorkId,
MapWorkAssessmentProposed, separate grade/estimate types, freshness and a
work_assessment_basis helper. A crate-private source loader must support a
caller-shared aggregate read budget. Card construction and assessment ingress
use the same Work/current-contract interpretation and fingerprint.

Existing selected_active_contracts semantics retain the last active contract in
the complete CONTRACT_WORK_ALL index order. Multiple active rows are not
automatically an error under that existing policy. Preserve it, rather than
switching to first, highest version or blanket duplicate-active refusal.
Duplicate/corrupt identities remain a separate integrity refusal.

The first descriptive-assessment scope is materialized Work, including group
and gate kinds. Resource and other cards reuse their available source data and
explicit missing/reference-only state. No resource availability or reverse
membership is inferred from a name. Complexity/difficulty estimates do not
become dispatch or authority decisions.

## Required evidence

- A real registered query/store reads exact plan membership and existing objects,
  including later pages, empty filtered continuation, absent Work and typed
  relation meanings. Wrong strategy/store/base/revision/query cursor refuses.
- A real command/store persists the optional assessment without changing Work or
  its authority; validates deserialized ranges and grade assumptions; checks
  record CAS, exact retry, relevant Work/contract drift, unrelated changes and
  cold reopen. The preserved multiple-active selection policy has a fixture.
- Route fixtures distinguish unique shared work, parallel prerequisites, missing
  or stale estimates and stated precedence-only bounds from a feasible schedule.
- Actual generic machine, authenticated HTTP and CLI entrypoints exercise the
  registered query. A DTO or registry-count assertion alone is insufficient.
- The development pilot conserves every original ID and both dependency kinds;
  its grouping creates neither execution barriers nor acceptance authority.
- Final changed-boundary tests, strict lint/format, conformance, public-use
  coverage and deterministic source traceability pass. New package source stays
  independently buildable and excludes development, private data and targets.

Workers write their own bounded checkpoints/reports in this directory. Root
updates plan.json and uniquely named SM-ROOT checkpoints. A transient worker
failure resumes its existing files; it does not create a formal handoff or
justify switching to a prohibited launcher/model lane.
