# Parallel plans, workspaces and deterministic simulation

Status: repository/product-runtime implementation is present. The final
zero-inference corpus passes 14/14 while reporting 50 explicit coverage gaps
(`coverageComplete = false`). All four providers completed the bounded main
question/Pause/wake/context flow. Codex/Luna and Claude Code/Haiku also observed
live Stop; Qwen/free and OpenCode retain historical cleanup failures from the
old exit ordering, followed by four passing public no-model tests of the exact
shared Stop repair. Contracts: PROP-012, PROP-013 and PROP-014.

## Intended result

One registered project can run several independent development plans, each with
its own coordinator, conversation and root worktree. A plan can give a worker an
isolated child worktree. Zap prepares these directories before launch and records
the assignment. Integration is prepared and reviewed separately, with conflicts
handled as an explicit task. All Lens clients consume the same shared state;
Quick Lens displays plan selection, responsibility, forks and integrations.

Keep reusable deterministic scenarios beside the corresponding behavioral tests.
Run them collectively or individually, repeat with recorded seeds, and retain
enough evidence to replay a failure without an LLM. Coverage gaps remain visible.

The developer-source runner is:

```text
npm run simulate:mock -- --list
npm run simulate:mock -- --id <scenario-id> --repeat 3 --seed <seed>
npm run simulate:mock -- --tag <tag>
npm run simulate:mock -- --coverage
npm run simulate:mock -- --all
```

Unknown IDs/tags fail. Each document names a fixed registered runner; scenario
data cannot supply a command. A receipt records scenario, runner, evidence kind,
iteration, effective seed and failing input position. Passing selected checks
does not imply complete product coverage. Gaps remain explicit unless
`--require-complete-coverage` is deliberately selected.

## Delivery sequence

1. Finish mock contract integration. Prove coordinator question, idle, Pause,
   retained human answer, Continue, automatic wake and acknowledgement through
   actual local HTTP. Prove the same through the actual managed backend, Node PTY
   and generated MCP. No hand-injected acknowledgements or success state.
2. Add the shared scenario corpus, schema validation, selection, repetition,
   replay receipts and coverage index. Separate pure reducer, local protocol,
   actual process, actual Git and provider-wire evidence.
3. Implement the repository workspace service: host-local repository binding,
   durable preparation, project-relative cwd, owned root/child worktrees,
   separate integration checkout, conflict state, review and checked promotion.
4. Compose additive plan contexts, broker scopes, coordinator launch and managed
   assignment with that service. Context selection must govern coordinator,
   planning source, model policy, history and cwd consistently.
5. Add shared read/command DTOs and Quick Lens plan/workspace controls, cards and
   graph fork/integration nodes. Preserve exact project and context in navigation.
6. Run the colocated corpus against disposable real Git repositories and mock
   agents, including clean merge, conflict, stale/dirty target and recovery.
   Then run bounded cheap real-provider checks for the original four-provider
   Pause/background-wake request. Mock results cannot prove vendor compatibility.
7. Review actual changes, run relevant final gates, inspect the UI, package and
   commit the accepted work. Keep unfinished scope explicit.

## Architecture decisions

- Repository, project, development-plan context, algorithm-plan revision,
  physical worktree, actor and execution attempt are separate identities.
- The common Git directory is a discovery key on one host. Stable opaque IDs and
  protected host bindings support later additional computers and clones.
- A development-plan context can be ready before an algorithm plan is adopted;
  the coordinator must be able to start and create that plan.
- Each command is scoped and attributable to a principal. Author, reviewer,
  account reference, agent, session, host and process epoch are distinct.
- Current execution is local-only. Remote execution, multi-user authentication,
  federation and distributed locking are future work, with explicit seams rather
  than claims of current support.
- The longer-term goal is crowdsourced execution capacity from participants'
  own computers and accounts. Resource contribution grants no merge authority.
  Preserve assignment/artifact/evidence provenance and independent review;
  a future trusted team-lead agent assists admission, while the server enforces
  its authority. Git worktrees are not an isolation boundary for hostile code.
  Contributor admission, sandboxing and resource scheduling are future work.
- Integration does not alter the target until a reviewed candidate passes the
  target HEAD and clean-worktree checks under an exclusive owned-writer gate.
  A changed basis requires a fresh attempt. No automatic stash, reset, force-push
  or worktree deletion is part of this delivery.
- Preparation, conflict, test, review and promotion are durable events. Successful
  Git operations do not by themselves accept semantic work.

## Required practical scenarios

- Two plans in one repository with independent contexts/coordinators and preserved
  monorepo subdirectory cwd.
- An isolated worker starts only after its worktree is ready; another plan and
  the original checkout remain unchanged.
- Clean integration is prepared separately and promoted to the expected target.
- A real Git conflict becomes a resolution task in the integration checkout;
  resolution follows the same review and promotion path.
- Dirty/stale target refusal and a retry after server restart do not duplicate
  worktrees or overwrite data.
- Map objects, runs and actors resolve to the recorded context/workspace; two
  contexts of the same project do not collide in graph keys.
- Existing question, lifecycle, work-report, review, note/Trash, model-policy and
  multi-project behavior is covered by reusable scenarios or recorded as a gap.

## Current findings

The additive runtime now supports several development-plan contexts in one
registered repository. The original checkout is metadata-adopted in place;
additional top-level plans receive owned root worktrees, conversation/broker
scope, coordinator launch options and protected cwd. Managed isolated-child
assignment records exact task/run/actor provenance before launch and resumes in
the same worktree.

Repository reads project current committed HEAD without changing durable state;
prepare commands CAS-record it. Integration artifacts and resolution work belong
to the source plan even when the recorded promotion target is the original
checkout in another context. The target context supplies writer activity and an
exclusive host-local lease. Bounded diff, registered test evidence, human review
and promotion are distinct. Plan/worktree/integration objects participate in
the shared notes/Trash resolver without borrowing semantic-snapshot coverage.

The ordinary launcher enables the local repository host and consistency profile.
A protected Git identity is optional for discovery/preparation and required only
when a commit must be authored. Planning-source attachment is per context and
identity-checked; protected dynamic attachment config is not persisted, so an
unrestored restart becomes explicitly unavailable/pending and requires
reconnection.

Current evidence includes 14 registered corpus scenarios: pure reducer cases, authenticated coordinator and
managed ZapMock flows, actual Node PTY/generated MCP cases, real temporary Git
clean/conflict/stale/recovery cases, the authenticated repository HTTP product
flow, notes and model-policy baseline scenarios, and graph/client projections.
The joined source gate records all five TypeScript configurations passing, Node
274 passed/zero failed/one explicit skip, tooling 2/2, Vitest 24/24 across nine
files, and clean lint, format, browser boundary and build. Conform reports zero
findings across 46 gated cells with zero exemptions. Specmap reports 153 units,
489 tags, 562 edges, zero suspect links, zero orphan roots and 16 visible
warnings. Focused provider, MCP credential, proxy-inheritance and raw-exit
lifecycle checks also pass.

The actual shared-canvas review rendered the current three-context sample with
14 selectable nodes in both light and dark themes. Context names, worktree lanes
and cross-context integration remain deterministic; label/lane corrections were
reviewed against the real 1600×1100 product view rather than a synthetic graph.

Remote hosts, distributed writer locking, multi-user identity and merge policy,
contributor admission, hostile-code isolation, resource scheduling and
crowdsourced computers/accounts remain future architecture. Native provider
children retain provider-owned cwd capabilities; only managed work has the
general isolated-worktree assignment contract today.
