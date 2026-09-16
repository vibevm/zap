# Parallel plans, workspaces and deterministic simulation

Status: active implementation. The previously accepted product baseline is
`110c2c43`. Later four-provider and mock changes are candidates until the checks
below pass. Contracts: PROP-012, PROP-013 and PROP-014.

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

The store and coordinator claims already support multiple contexts internally.
Product setup exposes only the default context. Additive registration, exact
context coordinator lookup, managed cwd assignment and context-qualified graph
keys are required. A real managed mock test also exposed a missing production
connection between the managed control runtime and the durable wake service;
helper tests alone did not detect it. These are implementation work, not accepted
completion claims.
