# ZAP-E1 transport implementation report

Status: repaired after independent review; candidate for root rerun.

## Result

Implemented the Python 3.11 standard-library `ProcessTransport` and independent
one-job `worker_host`. The adapter uses exact argv execution, a whole-argument
`{packet_file}` substitution, confined real working directories, a deliberately
small environment, immutable descriptor fingerprints, per-job nonces, durable
launch/start/completion receipts, OS process-start identities, cooperative stop
files, separately enabled exact-process termination and conservative recovery
from ambiguous launch.

Raw stdout/stderr are written to private partial files, flushed, atomically
published, hashed, and then referenced by the atomic completion receipt. Public
submit/status/stop/result/recovery records contain no argv, packet body,
environment values or output bodies. Private stderr is classified into success,
ordinary process failure, provider error, provider rate limit and provider quota
wait metadata; the transport emits no architectural verdict or product
acceptance.

## Stable interface decisions

The exact interface was frozen before implementation in `TRANSPORT-API.md`:

```python
ProcessTransport(root, allowed_workspace_roots, *,
                 inherited_environment=None, environment_allowlist=(),
                 allow_process_termination=False)
submit(job_id, *, argv, cwd, packet, environment=None) -> dict
status(job_id) -> dict
request_stop(job_id, request_id, *, mode="cooperative", terminate_after_seconds=None) -> dict
collect(job_id) -> dict
reconcile(job_id) -> dict
```

`packet` is UTF-8 text and exactly one argv element must equal
`{packet_file}`. Reuse of a job ID with an identical canonical logical
descriptor is idempotent; any changed argv, cwd, packet hash or effective
environment refuses with `IDEMPOTENCY`. Known pre-effect prepared work resumes
once on identical submit; starting or ambiguous work is never relaunched.
`collect` returns `ready: false` rather than treating
an active or unknown-effect operation as a result. Stop request persistence,
host delivery, process termination attempt and terminal exit remain separate
facts. Independent review added explicit termination capability/mode/delay,
`interrupted`, and separate delivery/termination/safe-state records so a
transport exit cannot be mistaken for a verified task safe boundary.

## Verification

Final command, run from the package root:

```text
python -B -m unittest discover -s 'vibevm/vibespecs/skills/zap-state/scripts' -p 'test_transport*.py' -v
```

Final result: exit `0`; `Ran 18 tests in 12.763s`; `OK`; zero failures,
errors or skips.

The tests use temporary local Python fixtures and real processes. They cover:

- successful packet/argv echo and literal hostile-looking arguments, including
  a non-whole `{packet_file}` substring;
- exact nonzero exit plus bounded provider-rate-limit classification;
- worker-host spawn failure with atomically finalized empty artifacts;
- reopening an adapter over a completed receipt;
- survival and completion after the submitting coordinator process exits;
- eight concurrent submissions plus a later retry starting exactly one child;
- changed descriptor refusal under a fixed job ID;
- durable stop delivery observed while the process is active, followed by
  cooperative terminal `stopped`, no termination, and an idempotent retry;
- explicit opt-in termination producing `interrupted` and
  `safe_state.needs_reconcile`, with changed-request refusal;
- self-created signal refusal and late post-exit stop preservation;
- known pre-effect descriptor-only crash recovery with exactly one launch;
- exact per-job artifact and host/start causal-chain validation;
- Windows environment case canonicalization and expanded credential-name refusal;
- missing host receipt reconciliation as `unknown_effect` with no relaunch;
- outside-workspace and Windows junction/reparse escape refusal; and
- absence of owner credentials from child input and absence of packet,
  environment and credential values from public records.

An intermediate expanded run exposed a Windows test-cleanup race because the
finished host briefly retained the temporary transport root as its current
directory. The host launch directory was changed to its module directory; the
same expanded suite then passed cleanly.

No network, external service, model call, repository boot, product/NEXT
execution or non-target test panel was used.

Final non-mutating workspace checks: scoped `git status --short` listed exactly
the eight paths below, all as ordinary untracked files; the targeted trailing-
whitespace scan had no matches. The overall worktree also contained unstaged
domain/control/knowledge/storage paths owned by the other packet workers. Those
were preserved without edits, deletion, staging or attribution.

## Changed paths

- `vibevm/vibespecs/skills/zap-state/scripts/zaplib/transport.py`
- `vibevm/vibespecs/skills/zap-state/scripts/zaplib/transport_io.py`
- `vibevm/vibespecs/skills/zap-state/scripts/zaplib/transport_receipts.py`
- `vibevm/vibespecs/skills/zap-state/scripts/zaplib/worker_host.py`
- `vibevm/vibespecs/skills/zap-state/scripts/test_transport.py`
- `vibevm/vibespecs/development/zap/TRANSPORT-API.md`
- `vibevm/vibespecs/development/zap/REPORT-E1.md`
- `vibevm/vibespecs/development/zap/REVIEW-E1.md`

All are inside the transport write perimeter. Implementation modules remain at
or below 500 lines (`transport.py` 499, `transport_io.py` 282,
`transport_receipts.py` 260, `worker_host.py` 290).

## Limitations

- This is transport confinement and receipt ownership checking, not an OS
  sandbox against a deliberately malicious same-user child or administrator.
- Cooperative cancellation is the default. Explicit termination targets only
  the exact verified process; it neither walks a process tree nor proves a task
  safe boundary or rolls back arbitrary external effects. `interrupted` always
  needs runtime reconciliation.
- Live ownership uses Windows process creation time or Linux `/proc` start time.
  Platforms without a reusable process-start token conservatively report
  `unknown_effect` rather than claiming ownership.
- Provider classifications are bounded transport metadata inferred privately
  from stderr markers; they are not semantic or architectural judgments.
- Credential-shaped environment names are refused entirely. A future worker
  needing a provider secret requires a separately designed secret-delivery
  channel rather than widening this public environment interface.
- Real-process verification ran on Windows. Unix session and `/proc` paths are
  implemented but were not executed in this packet environment.
- On Windows the transport root must inherit a restrictive account ACL;
  Python `chmod` does not configure a DACL. Receipt binding is not
  authentication against a hostile same-user writer with root access.
