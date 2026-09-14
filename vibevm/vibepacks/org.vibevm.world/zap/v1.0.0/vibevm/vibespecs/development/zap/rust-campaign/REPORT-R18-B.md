# R18-B core, legacy, and API gate report

Status: bounded production-floor candidate; root review required. Four of nine
product crates are now gated. R18 remains open.

## Core

The genuinely gated core baseline was 48 findings: 47 missing seam doctests and
one oversized packet module. R17 split the packet module without changing its
public surface. R18-B added compiled contracts to every reported seam.

The final rustdoc run passes 45 positive examples and two compile-fail refusal
examples. They cover canonical record keys and versions, immutable state,
bounded queries/traversal, artifact witnessing, basis derivation, completion,
admission, effect simulation, transaction/replay ports, typed transition
adapters, packet material/workspace verification, credential authority, secret
checking, and the AgentHost effect boundary. The two refusals demonstrate that
StateReader exposes no write path and CommandPayload cannot omit canonical
encode/decode contracts.

The examples add no fixture API, credential constructor, authority bypass,
blanket allowance, or frozen finding. Transition's erased adapter moved into a
private sibling module to retain the 600-line boundary; sibling construction
visibility remains `pub(super)`.

Final core receipts:

- `run-cargo.ps1 test --doc -p zap-core`: 47/47 passed.
- `run-cargo.ps1 clippy -p zap-core --lib --no-deps -- -D warnings`: exit 0.
- Genuinely gated candidate conform: 0 findings, 0 frozen, 0 new.
- Actual `rust-ai-native conform check --scope crates/zap-core/`: 0 findings,
  0 frozen, 0 new; core added to `rust.gated`.

The R18-A core unit suite remained unchanged and its 7/7 receipt is reused.

## Legacy

The genuinely gated legacy candidate reported zero findings. No product code or
schema was changed. R14's accepted migration, recovery, and frozen-reader
receipts remain the behavioral evidence.

- Actual `rust-ai-native conform check --scope crates/zap-legacy/`: 0 findings.
- `run-cargo.ps1 clippy -p zap-legacy --lib --no-deps -- -D warnings`: exit 0.
- `zap-legacy` added to `rust.gated` with no baseline entry.

## API

R16 released the stable command DTO surface. The gated candidate reported one
finding on `MachineReadPort`. Its compiled example now verifies that each
supported read request maps to its matching typed response without exposing
mutation authority.

Strict API clippy also exposed a real large-enum finding: the inline
`CandidateResult` made `NativeDriverRequest::RecordCandidate` at least 520
bytes. R16 changed that field to `Box<CandidateResult>` and updated its app
consumer; serde's transparent Box representation preserves public JSON.

- `run-cargo.ps1 test --doc -p zap-api`: 1/1 passed.
- `run-cargo.ps1 clippy -p zap-api --lib --no-deps -- -D warnings`: exit 0.
- Actual `rust-ai-native conform check --scope crates/zap-api/`: 0 findings.
- `zap-api` added to `rust.gated` with no baseline entry.

## Current floor and accounting

`conform.toml` now gates `zap-api`, `zap-core`, `zap-legacy`, and `zap-wire`.
Five product crates remain explicitly exempt. The conformance baseline remains
empty.

The current health snapshot reports 13 files over 600 lines and 17 in the
540–600 danger band. The higher danger count is expected after cohesive pieces
moved below the hard limit; it is not counted as closed debt. Public-type
doctest gating is still empty and remains separate work from the seam contracts
drained here.

Debt entries for core, legacy, and API are marked fixed with their exact
receipts. Nine other debt entries and all five carry-over intents remain open;
`rust-ai-native ledger render --check` confirms the rendered views match.

R17 is still changing mapped viewer/index source, so specmap regeneration and
orphan adjudication remain deferred to its stable-source signal. The earlier
mandatory live native-LLM gate was removed by the owner; R18 uses accepted
algorithmic receipts and R16's deterministic nonempty-close/recovery oracles.

Remaining crate gates are app, CLI, domain, runtime, and store. No whole-R18,
package, retirement, or publication acceptance is claimed.

No live model, external runner, full host panel, Git operation, publication,
online install, host repair, baseline widening, or unsupported metadata command
ran in R18-B.
