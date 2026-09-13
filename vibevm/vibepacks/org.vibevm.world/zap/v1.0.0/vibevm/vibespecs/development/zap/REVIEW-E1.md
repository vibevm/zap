# Independent review — ZAP E1 subprocess transport

## Repair disposition

All actionable findings below were repaired in the bounded E1 follow-up.

- **Self-signal termination — resolved.** The host polls exact descriptor-bound
  request records; `stop.signal` only informs the child. A child-created signal
  with no request exits naturally and never creates delivery or termination.
  Evidence: `test_child_cannot_self_authorize_stop_by_writing_signal`.
- **Late stop reclassification — resolved.** Deliveries are written only while
  the child is observed live. A request created after OS exit remains visible
  and undelivered while the natural succeeded/failed completion is retained.
  Evidence: `test_late_stop_after_child_exit_does_not_reclassify_or_deliver`.
- **Unsafe force semantics — resolved.** Cooperative-only is the default.
  Termination needs constructor capability, explicit request mode and a
  positive caller delay; its terminal state is `interrupted`. Public delivery,
  termination and safe-state objects are separate, and transport never claims
  safe-state verification. Evidence:
  `test_process_termination_requires_explicit_capability_and_is_interrupted`
  and the repaired cooperative stop test.
- **Prepared liveness — resolved.** Identical submit completes and launches a
  descriptor-only or valid-prepared pre-effect record under the transport lock.
  Any launch intent or other artifact retains the no-relaunch rule. Evidence:
  `test_known_pre_effect_prepared_state_resumes_once` plus the existing
  ambiguous-launch test.
- **Cross-job artifacts/receipt chain — resolved.** Completion accepts only the
  owning job's exact stdout/stderr paths and exact record shapes, and normal
  completion requires the descriptor-bound host/start chain. Evidence:
  `test_completion_cannot_claim_another_jobs_artifacts` and
  `test_started_receipt_without_host_chain_does_not_prove_ownership`.
- **Credential-shaped names — resolved.** PAT, COOKIE, BEARER and PRIVATE_KEY
  token families join the existing deny set without rejecting `PATH`.
- **Windows environment aliases — resolved.** Names are canonicalized before
  effective-environment construction, hashing and spawn; duplicate submitted
  aliases refuse. Evidence:
  `test_windows_environment_case_is_canonical_before_hash_and_spawn`.

Final focused result: `Ran 18 tests in 12.763s` — `OK`. Receipt validation was
split into `transport_receipts.py`; all four implementation modules remain at
or below 500 lines. The documented same-user/administrator and non-Linux Unix
limitations remain accepted scope boundaries rather than implementation claims.

Review scope was limited to the frozen transport API/runtime contract, the
three transport implementation modules, and their focused tests. Probes used
temporary local Python processes only. No network, model, live task, Git
mutation, or product edit was performed.

The existing 11 transport tests pass on Windows (`Ran 11 tests in 4.876s`,
`OK`). Literal argv execution, coordinator-exit survival, normal concurrent
idempotency, changed-descriptor refusal, normal completion reopening, missing
host reconciliation, path confinement, and ordinary public-record redaction
behave as reported. The following material issues remain.

## [P1] The child-controlled stop path can terminate work without any stop request

`worker_host.run` gives the child the exact `ZAP_STOP_FILE` path
(`worker_host.py:217-218`), then treats mere existence of that file as a stop
at `worker_host.py:252-258`. It calls `safe_terminate` after 0.75 seconds
without first requiring a valid descriptor-bound stop-request record. At
`worker_host.py:270-275` the same file alone changes a natural/forced exit into
terminal `stopped`.

Temporary-process reproducer: the child itself wrote its advertised
`ZAP_STOP_FILE` and slept for three seconds; the caller never called
`request_stop`. The process was terminated after about one second and collect
returned:

```text
state=stopped, exit_code=1, termination_sent=true
stop.requested=false, stop.request_count=0, stop.delivered=false
```

This is outside neither path confinement nor PID ownership: the adapter kills
the correct child, but does so without an authorized/durable cancellation
request and then reports a stop that never existed.

Required fix: the signal file must be only a wake-up mechanism. Delivery,
termination and stopped classification must require at least one valid
descriptor-bound request returned by `stop_requests`. A stray/self-created
signal should be ignored or surfaced as corrupt/untrusted input and must never
trigger termination.

## [P1] Stop arrival after OS exit is falsely delivered and changes success into stopped

`request_stop` decides that work is nonterminal only by the absence of
`completed.json` (`transport.py:413-438`). The child may already have exited
while the host is flushing or publishing artifacts. After `process.wait`
returns (`worker_host.py:260`), the host still checks the signal at
`worker_host.py:270-275`, creates delivery records and classifies the completed
operation as stopped.

Deterministic probe: `publish_file` was blocked after a zero-exit child had
already returned; `request_stop` was then called before allowing completion
publication. The immediate stop result correctly said
`delivered=false, actual_exit=false`, but the final result became:

```text
state=stopped, exit_code=0, diagnostic=stop_completed
status.stop.delivered=true, termination_sent=false
```

The request did not reach a live child and did not cause its exit. This can
mislead effect reconciliation and owner pause reporting.

Required fix: capture the child-exit observation time immediately when poll or
wait first reports exit. Only a valid request observed while the child was live
may be delivered to it or determine stopped state. A request racing after exit
should remain a late, undelivered request while the original succeeded/failed
result is preserved.

## [P1] `stopped` is not a safe-boundary result, and default force termination is unconditional

The host sends the cooperative file and, after a fixed 0.75 seconds, calls
`Popen.terminate` for the verified child (`worker_host.py:250-259`). Neither the
descriptor nor the stop request contains the task's declared safe boundary or
a cooperative acknowledgment. Completion collapses every exit after a signal
to `stopped` (`worker_host.py:270-287`).

The shipped `test_stop_delivery_is_separate_from_actual_exit` is direct
evidence: its child observes the cooperative signal, deliberately remains live,
is forcibly terminated, and collect returns `stopped` with
`termination_sent=true`. This proves exact-process termination, but it cannot
prove a safe checkpoint or reconciled external effects.

Required fix: freeze an explicit cancellation policy per job/request
(`cooperative_only`, or an authorized force deadline). Report cooperative
acknowledgment, force attempt, process exit and safe-boundary/effect
reconciliation as separate facts. A forced exit must remain
`interrupted/needs_reconcile` (or an equivalently explicit result) until the
runtime proves the task's declared safe boundary. Runtime E must never map the
current `stopped` value directly to safe drain.

## [P1] A proven pre-effect crash boundary is permanently stranded

Submission publishes `packet.txt` and `descriptor.json` before
`prepared.json`, `launch_intent.json` and host spawn
(`transport.py:203-207`). On an identical retry, descriptor existence takes the
idempotent branch and skips every missing publication/launch step
(`transport.py:185-188`). Status then reports `prepared`
(`transport.py:392-393`), while `reconcile` promises never to relaunch
(`transport.py:490-494`).

Injected crash at the `prepared.json` write produced exactly two job files,
`descriptor.json` and `packet.txt`. Status, retry and reconcile all remained
`prepared`; retry returned `accepted=true, idempotent=true`; no process ever
started and there is no API operation that can progress it.

This state differs from an ambiguous spawn: protocol order proves there is no
launch intent and therefore no possible host claim. Conservative non-relaunch
is correct after launch intent, but loses liveness before it.

Required fix: under the transport lock, an identical submission may complete
the missing prepared/launch-intent records and launch exactly once only when no
launch intent, parent/host/start/completion receipt, or stop exists. Preserve
the current no-relaunch rule from launch intent onward. Add crash tests at each
publication boundary.

## [P1] Completion artifacts are verified against the whole transport root, not the owning job

`_completed` calls `verify_artifact(..., self.root)` for stdout and stderr
(`transport.py:327-341`). `verify_artifact` accepts any matching regular file
below that root (`transport_io.py:130-140`). It does not require the exact
owning `job_dir/stdout.bin` and `job_dir/stderr.bin` paths.

Probe: two jobs A and B completed normally. A's descriptor-bound
`completed.json` was changed to contain B's valid stdout/stderr artifact
records. `collect('A')` returned `ready=true, state=succeeded`, exposed B's
stdout path/content (`bravo`), and `status('A').process.ownership_verified`
was true. The same weakness can point an output record at another private file
inside the transport root.

Required fix: completion validation must require exact resolved artifact paths
inside the owning job directory, reject links/reparse components, and validate
the receipt's exact shape and causal chain. A verified completion should link
to the matching started/host evidence (with a separate valid spawn-failure
case); root-wide file membership is not job ownership.

Related threat-boundary finding: a forged `started.json` containing copied
job/nonce/descriptor fields plus the current process PID/start token is accepted
as `running, ownership_verified=true` at `transport.py:354-363`, even when no
host was launched. The nonce is stored beside the receipt and is not an
authenticator. Preventing a hostile same-user writer requires a stronger
host-only write boundary or authenticated receipts and is outside the current
documented threat model. The API and backend must describe this honestly; the
cheap exact-path/causal-chain checks above are still required for corruption,
cross-job mixups and non-hostile faults.

## [P2] Common credential-shaped environment names are accepted despite the contract

`validate_environment_name` rejects only substrings in
`_CREDENTIAL_WORDS` (`transport_io.py:16-18,143-153`). Construction succeeded
with each of these explicit allowlist names:

```text
GITHUB_PAT, SESSION_COOKIE, BEARER, PRIVATE_KEY
```

The frozen API says credential-shaped names are always refused even when
listed. The values remain absent from public receipts, but they are accepted,
stored in the private descriptor and delivered to the child, contrary to that
contract.

Required fix: either expand and test the denied credential-name vocabulary
(including PAT, COOKIE, BEARER and PRIVATE_KEY families) or narrow the public
promise to an explicit, versioned deny list. Keep provider secrets on the
separate secret-delivery path.

## [P2] Windows environment casing breaks semantic idempotency

Allowlist membership is case-folded on Windows, but `_effective_environment`
stores the caller's original spelling (`transport.py:97-112`). With inherited
`PATH`, allowlisted `PATH`, and submitted `{'Path':'X'}`, the private descriptor
contained both `PATH` and `Path`; the child observed one `PATH=X`. Retrying the
same logical Windows environment as `{'PATH':'X'}` failed with `IDEMPOTENCY`
because the canonical descriptor changed.

Required fix: canonicalize Windows environment keys before merge and hashing,
including inherited/allowlist overlap. Equivalent Windows environments must
produce one effective mapping and one descriptor fingerprint.

## Platform and boundary notes

- Literal argv and whole-argument packet substitution are correctly
  implemented with `shell=False` behavior (`transport.py:123-136`,
  `worker_host.py:201-235`) and passed the hostile-string test.
- Coordinator exit survival and eight-way submit contention passed. Missing or
  PID-reused start tokens degrade to `unknown_effect`; no blind relaunch or
  name/tree kill was found.
- Process-tree cancellation is explicitly unsupported and the code terminates
  only its exact verified `Popen` child. Descendants and arbitrary external
  effects therefore require runtime reconciliation after child exit.
- Linux `/proc` start-time and Unix session/lock paths are plausible but were
  not executable in this Windows review. macOS/other Unix intentionally cannot
  verify a reusable start token and will report `unknown_effect`; that is an
  honest limitation, not a defect.
- Unix modes are tightened to 0700/0600. Windows `chmod` does not establish a
  private DACL. If `root` can inherit a permissive ACL, private descriptors may
  contain environment values accessible to other local principals. Either the
  Windows constructor must verify/create a protected directory or the API must
  require and state that precondition. This is a platform-security limitation,
  not proof of a public-record leak in the current tests.
- No cleanup/delete API exists, so the reviewed code cannot delete foreign job
  data. Partial artifact files remain private and yield conservative
  nonterminal/unknown behavior; the pre-intent liveness issue above is the
  material crash-recovery defect.
