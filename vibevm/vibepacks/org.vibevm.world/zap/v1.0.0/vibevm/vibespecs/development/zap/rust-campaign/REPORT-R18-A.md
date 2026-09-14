# R18-A production-discipline report

Status: useful first batch complete; root review required. This is not the R18
package/release gate.

Observed boundary: 2026-09-14T05:28:41.5270243Z.

## Result

`zap-wire` is the first product crate to leave pre-adoption. Its two real
conformance findings were repaired with compiled `CanonicalEncode` and
`CanonicalDecode` examples. The crate's 15 runnable tests, two doctests, lint,
and exact path-scoped conformance are green. The conformance baseline remains
empty.

All five R18-owned oversized core parents are now below 600 lines. The split is
structural: public paths, canonical data, route decisions, error requirements,
and authority semantics are unchanged. Construction fields exposed across
sibling modules use only `pub(super)` visibility.

| Parent | Before | After | Extracted responsibilities |
| --- | ---: | ---: | --- |
| `commit.rs` | 2,138 | 311 | logical events, index planning, event replay, builder, service, validation |
| `transition.rs` | 1,404 | 495 | payload adapters, erased/typed cells, tests |
| `trust.rs` | 934 | 550 | credential authority/grants, tests |
| `preflight.rs` | 824 | 404 | effect-bundle derivation |
| `effects.rs` | 773 | 557 | drafts, tests |

The largest new owned child is `transition/cell.rs` at 594 lines. Existing core
tests pass 7/7 after the complete split.

Core's live file-length findings fell from six to one. The remaining 759-line
`execution_views/packet.rs` is in R17's active ownership. A temporary candidate
policy that actually gates core reports 48 remaining findings: that one file
and 47 public seam traits without compiled doctests. Core therefore remains
exempt, and no fault was frozen into `conform-baseline.json`.

## Policy and registries

`conform.toml` now gates `zap-wire` and retains living exemptions for the other
eight product crates. `crates/*` discovers direct product crates; the nested
`crates/vendor/` sources are classified as byte-verified external inputs by the
policy comment and their existing exact `crates/vendor/PROVENANCE.md` identity
and hashes.

`specmap.toml` now names the nine product crate roots explicitly. Product code
contains only the ZAP namespace; the external ai-native citations are confined
to vendored upstream tests, which are outside the product scan. Removing those
unnecessary external roots also makes the policy valid in both the source tree
and Vibe's installed sibling-dependency topology.

The brownfield registries now contain:

- 12 open debt entries with exact evidence, owner-path tripwires, and sunsets;
- 5 open carry-over intents covering crate gates, specmap, environment audit,
  final package evidence, and permanent migration records;
- 15 exact passing nextest identities for the first gated wire crate, with the
  two R17 measurement probes recorded as intentional skips rather than failure
  exemptions.

`rust-ai-native ledger render --check` confirms `discipline/DEBT.md` and
`discipline/INTENT.md` match the JSON registries.

## Checker evidence

The package-local CLI was built through Vibe's explicit dependency-build
consent gate and its actual help surface was inspected. It exposes the required
conform, specmap, test-gate, tripwire, health, floor, fast-loop, and ledger
commands.

The `conform check --scope` implementation consumes a repository path prefix,
although help describes the value as a crate name. Bare `zap-wire`/`zap-core`
scopes returned misleading zeroes. All evidence below uses
`crates/zap-wire/` or `crates/zap-core/`, after a temporary candidate policy
made the crate genuinely gated.

- Initial wire candidate: 2 `seam-has-doctest` findings.
- Final wire gate: 0 findings, 0 frozen, 0 new; one crate gated, eight exempt.
- Initial core candidate: 53 findings, comprising 6 file-length and 47
  seam-doctest findings.
- Final core candidate: 48 findings, comprising 1 file-length and 47
  seam-doctest findings.
- Current health snapshot: 17 files over budget and 14 in the danger band,
  down from 20 and 11 respectively. The danger count rose because decomposition
  made three formerly oversized responsibilities visible below the hard limit;
  it is not presented as debt elimination.

## Commands and receipts

- `run-cargo.ps1 test -p zap-wire`: exit 0; 7 unit and 8 integration tests
  passed, 2 explicit R17 probes ignored, and 2 doctests passed.
- `run-cargo.ps1 nextest run -p zap-wire --status-level all`: exit 0; 15/15
  runnable tests passed and 2 probes skipped.
- `rust-ai-native conform check --scope crates/zap-wire/`: exit 0; zero
  findings after the gate flip.
- `run-cargo.ps1 test -p zap-core`: exit 0; 7/7 passed.
- `run-cargo.ps1 clippy -p zap-wire -p zap-core --lib --no-deps -- -D
  warnings`: exit 0.
- `rust-ai-native ledger render --check`: exit 0.
- `rust-ai-native conform check --scope crates/zap-core/`: expected exit 1;
  one remaining live file-length finding under the current policy.

One exploratory `cargo nextest list -p zap-wire` command was mistakenly invoked
without `run-cargo.ps1`. It succeeded using the package-local target and changed
no source. Its result is excluded from the gate evidence; the subsequent
authoritative nextest run used the required wrapper and shared target.

## Remaining work

- R17 owns the final oversized core packet module and is still changing mapped
  viewer/index source. At its request, specmap regeneration and the orphan gate
  are deferred until its final coherent boundary.
- Core still owes 47 meaningful seam doctests before it can be gated.
- The other seven product crates remain explicit pre-adoption debts; their
  current oversized modules and public contract gaps are in the registries and
  health snapshot.
- Public-type doctest gating, environment/audit roots, complete spec orphan
  dispositions, and the final stable installed package identity remain open.
- R16's root-owned native candidate-acceptance receipt remains independently
  pending and was not changed by this structural batch.

No full workspace floor, full host panel, native model invocation, NEXT access,
Git operation, public publication, host repair, online install, or baseline
widening occurred.
