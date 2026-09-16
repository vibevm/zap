# SM04 public strategic-map integration report

Status: candidate complete for root review.

The new isolated application target seeds a valid four-Work diamond and exact
StrategicPlan through a test-only CommitService, then closes that writer before
opening the real ApplicationService. The public `zap.map.object.v1` result
supplies the current assessment source fingerprint. The test submits a real
`MapWorkAssessmentProposed` through the configured Data channel with the
current global header revision and empty record-CAS expectation. It never calls
the assessment basis helper.

The same journey rereads the card through the generic MachineReadPort and an
authenticated HTTP `POST /v1/query`. It verifies Current assessment metadata,
the unchanged Work record and actual query registration. A two-page overview
preserves all four Work identities. A selected diamond-tip route returns the
three distinct prerequisites, counts the shared root once, reports three
missing estimates, aggregates exactly the supplied 1–2 agent-hour interval and
reports a 2-hour precedence lower bound without claiming a resource-feasible
schedule or a complete estimate.

After closing ApplicationService, the fixture performs one semantic Work
change through its test-only CommitService and closes that writer. A fresh
ApplicationService instance reopens the store in the same test process. Its Work card reports the prior
assessment Stale and exposes the new source fingerprint. The route lists the
target estimate as stale, excludes it from current agent-hour and elapsed
aggregates, and remains incomplete.

The separate zap-cli target invokes the compiled `zap` binary with the normal
`STORE.redb REQUEST.json` entrypoint. It decodes registered overview and object
query responses before and after a closed-writer Work update, preserving all
diamond identities and observing the changed source fingerprint. No CLI verb,
MachineRequest variant, HTTP route or production app/API/runtime source was
added.

Verification:

- `cargo test -p zap-app --test strategic_map_service --offline`: 1 passed.
- `cargo test -p zap-cli --test strategic_map --offline`: 1 passed.
- strict Clippy with `-D warnings` for each exact target: passed.
- coordinated domain/app/CLI formatting and `--check`: passed.

The first application attempt exposed only overlapping Redb ownership in the
fixture. The final fixture closes each seed/application writer before the next
instance reopens the store; the corrected public journey is green. All new test
files are below 600 lines and retain LF. No live model, graphical client,
activation, NEXT campaign, 1.0 source or published tag was touched.
