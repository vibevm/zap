# Zero-LLM simulation corpus

Behavior scenarios are colocated with the tests and cells they explain as
`*.simulation.json`. Every document has a stable scenario ID, a known runner
ID, deterministic seed, feature coverage, validated inputs and observable
expectations. Scenario data never contains an executable, argv or code string.

`coverage.json` is the explicit index. Discovery fails for an unindexed
colocated scenario, a missing indexed path, an identity mismatch or coverage
that references an unknown scenario. A coverage gap is reported as a gap; it
is not counted as a passing scenario.

Developer commands:

```text
npm run simulate:mock -- --list
npm run simulate:mock -- --coverage
npm run simulate:mock -- --id model.full-lifecycle --repeat 100
npm run simulate:mock -- --all
npm run test:mock
```

Use `--seed <seed>` to replay or vary a selected scenario. Repetition derives
an explicit seed suffix and creates fresh state for every iteration. A failure
receipt includes scenario ID, runner/evidence kind, exact seed, input position
and a replay command. `--all` executes every registered scenario and reports
`checksPassed` separately from `coverageComplete`. Add
`--require-complete-coverage` when missing feature coverage must fail the run.

Product runners are a closed source mapping. The harness starts only known test
entries with Node directly and no shell. It requires a structured receipt that
echoes the selected scenario ID and seed; exit code zero alone is insufficient.
Product processes must close naturally. A bounded timeout stops the owned child
and reports failure.
