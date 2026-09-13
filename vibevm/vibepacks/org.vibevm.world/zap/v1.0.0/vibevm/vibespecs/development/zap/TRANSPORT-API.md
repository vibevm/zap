# ZAP durable subprocess transport API

This document freezes the Python 3.11 standard-library subprocess boundary used
by the ZAP runtime. The transport owns process execution only. It does not
authorize product work, record architectural verdicts, or imply acceptance of a
worker result.

## Construction

```python
ProcessTransport(
    root: str | os.PathLike[str],
    allowed_workspace_roots: Iterable[str | os.PathLike[str]],
    *,
    inherited_environment: Iterable[str] | None = None,
    environment_allowlist: Iterable[str] = (),
    allow_process_termination: bool = False,
) -> ProcessTransport
```

`root` is a dedicated private transport directory. Each allowed workspace root
must be an existing real directory. A submitted working directory must be below
one of those roots, and every existing path component is checked for symlinks or
Windows reparse points. The transport confinement is a path and ownership
boundary, not an operating-system sandbox against a malicious executable.

`inherited_environment` is an explicit list of names copied from the host. When
it is `None`, a small platform-specific runtime list is used (`PATH`, locale and
temporary-directory variables on Unix; the corresponding Windows runtime
variables). `environment_allowlist` lists additional names callers may set in
`submit`. Credential-shaped names and ZAP owner/control/coordinator names are
always refused, even if listed. Environment names and values never occur in a
public receipt. Windows names are canonicalized before descriptor hashing and
spawning, so case aliases cannot create duplicate effective entries. The
worker receives a transport-owned `ZAP_STOP_FILE` in addition to the selected
environment. File existence is not authority: the host acts only on a valid
descriptor-bound request record.

`allow_process_termination` enables an explicit exact-process termination
capability for jobs created by that adapter. It defaults false and is part of
the immutable descriptor. Enabling it alone does nothing; a specific stop call
must also select terminate mode and a positive caller-supplied delay.

## Stable interface

```python
submit(job_id, *, argv, cwd, packet, environment=None) -> dict
status(job_id) -> dict
request_stop(job_id, request_id, *, mode="cooperative", terminate_after_seconds=None) -> dict
collect(job_id) -> dict
reconcile(job_id) -> dict
```

`job_id` and `request_id` are stable ZAP identities. `argv` is a nonempty
sequence of strings and is passed directly to `subprocess.Popen`; no shell is
created. `packet` is UTF-8 text stored in a private immutable artifact. Exactly
one argv element must equal `{packet_file}`. Only that whole element is replaced
with the private packet path. Packet text and hostile-looking argv strings are
never evaluated. `cwd` is an allowed real directory. `environment`, when
present, is a string-to-string mapping whose keys are in the construction-time
allowlist.

The logical descriptor consists of schema, exact job ID, argv template,
canonical cwd, packet SHA-256, effective child environment and process-
termination capability. Its canonical SHA-256 is the `descriptor_sha256`. A
fresh random nonce binds the durable artifacts for that descriptor. The
environment is retained only in the private descriptor; public records expose
neither its names nor values.

The first submission durably creates the packet, descriptor and launch intent,
then starts a detached/independent `worker_host`. An identical retry returns an
idempotent receipt and never launches another host or child. Reuse of the job ID
with any changed logical descriptor raises `Refusal("IDEMPOTENCY", ...)`. Once a
descriptor exists, a missing or ambiguous launch receipt is reconciled; it is
never repaired by blind relaunch.

There is one proven pre-effect exception. If a crash left only the immutable
packet/descriptor and optional valid prepared receipt, with no launch intent,
host/start/completion/stop or foreign artifact, an identical `submit` finishes
the prepared and launch-intent records under the transport lock and starts
once. Its receipt has `resumed_pre_effect=true`. From launch intent onward,
missing evidence remains ambiguous and is never relaunched.

## Records and states

All returned dictionaries are public, bounded records. They contain the job ID,
nonce, descriptor fingerprint and one of these states:

- `prepared`: immutable inputs exist and no launch intent is visible.
- `starting`: launch was intended or a verified host exists, but no child-start
  receipt is yet visible.
- `running`: a started receipt exists and the recorded PID plus OS process-start
  token still identify that child.
- `stop_requested`: at least one durable stop request exists and terminal exit
  is not yet recorded.
- `succeeded`: the child exited zero and no stop had been requested.
- `failed`: spawn failed or the child exited nonzero without a stop request.
- `stopped`: the child exited after a valid request was delivered while it was
  still observed live, without process-level termination. This does not prove
  the task's safe boundary.
- `interrupted`: explicit process-level termination was sent for the verified
  child. This always requires effect and safe-boundary reconciliation.
- `unknown_effect`: launch/start evidence exists but current ownership or final
  effect cannot be proved.

`status` is observational. Its `process` member reports `active` as true, false,
or null and separately reports whether ownership is verified. Its `stop` member
reports requested and delivered facts separately. A PID is never sufficient:
the nonce and descriptor fingerprint in a durable receipt must match, and live
process checks also compare an OS process-start token when the platform exposes
one.

`request_stop` atomically stores a request record and creates the cooperative
signal file. Cooperative mode is the default and never calls process
termination. Repeating the same request ID and policy is idempotent; changing
mode or delay under the same ID refuses. Terminate mode requires both the job
descriptor capability and a caller-supplied positive delay. The worker host
records delivery only while the child is observed live, then may terminate only
the specific child represented by its live `Popen` after verifying the complete
host/start chain, job ID, nonce, descriptor fingerprint, PID and process-start
token. It never kills by name or walks a process tree. A request observed after
OS exit remains undelivered and cannot change natural success/failure.

Status and collect expose separate `delivery`, `termination` and `safe_state`
objects. Transport always reports `safe_state.verified=false`; PID exit,
cooperative delivery and a terminal receipt do not prove a task checkpoint or
reconciled external effect. `stopped`, `interrupted`, `unknown_effect`, or any
terminal result with a late stop request has
`safe_state.needs_reconcile=true`. Runtime validates a task receipt or
postcondition before acknowledging the declared safe boundary.

`collect` returns `ready: false` for nonterminal or unknown-effect jobs. For a
verified terminal receipt it returns `ready: true`, exact exit status, bounded
diagnostic classification, and private stdout/stderr artifact records containing
only path, byte length and SHA-256. Raw output is never embedded. `reconcile`
derives the strongest state justified by durable receipts and current process
identity. It may report `unknown_effect`; it never launches work.

## Durability and limits

Descriptors, launch/started/completed receipts, stop requests, delivery records
and final artifact identities are written by atomic replace with file flushes.
Raw stdout and stderr stay at the owning job's exact `stdout.bin` and
`stderr.bin` paths. Completion validation checks exact receipt shape,
host/start chain, stop/termination references, hashes and those exact paths; a
different job or private file cannot be substituted. Windows hosts and
children use no-window creation flags; Unix hosts use an independent session.
Termination is process-level only: arbitrary external effects are not rolled
back. If the operating system cannot expose a reusable process-start token,
reconciliation remains conservative and does not claim live ownership.

On Windows the caller places the root below a directory whose ACL already
restricts access to the intended account. Python `chmod` is not a DACL
implementation. Receipt binding detects corruption and cross-job mixups; it is
not cryptographic authentication against a hostile same-user writer who can
read and modify the private transport root.
