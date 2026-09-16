# Root disposition of the environment-audit finding

DBT-R18-010's proposed remedy was based on an incorrect tool scope. The installed
Rust conformance contract RULE-AMBIENT-ENV and its actual frontend/rule inspect
std::env variable access (var, var_os, set_var, remove_var). The env_roots list
does not register filesystem, process or network adapters. Current supported
ambient checks report no non-root production access. Adding adapter paths to
that list would neither implement nor test a general effects audit.

The actual product boundary remains the permanent RUST-STORAGE-MODULE-BOUNDARIES
contract: pure transitions use injected read/command/provider interfaces;
filesystem, process and network effects belong to the application/store/CLI
adapters. Root inspected the application construction, material configuration,
bounded child process and Vibe-query binding, and reused the previously accepted
R13C service/lease and R14 import/recovery source reviews and execution receipts.
The production core/domain/runtime source search found no direct filesystem,
network, environment-variable, process or system-clock calls. This is source
inspection, not a new general-purpose analyzer.

| Effect boundary | Explicit inputs and established evidence |
| --- | --- |
| Store and artifact storage | Configured paths and store/record identities; transactional store, artifact witness, exact retry and reopen tests; R04/R14 accepted reviews. |
| Material/workspace/archive adapters | Named roots, grants, source digests, archive/output bounds and a configuration directory; R15A accepted adapter review and R13C packet/archive route proof. |
| Vibe query subprocess | Configured canonical executable, exact project path and URI, offline/agent arguments, output and elapsed-time limits; bounded-process and material-adapter tests. |
| Service lease and endpoint publication | Configured paths, exact store/owner bytes and retained store guard; R13C lease-recovery/foreign-content preservation review and service tests. |
| Loopback read/protected servers | Supplied address, service/store binding, credential file and request limits; read-server, protected-route and compiled-binary integration tests. |
| Packet captures and legacy import | Configured capture/source/destination paths, manifest/receipt hashes and bounded publication/recovery protocols; R13C and R14 accepted evidence. |
| Native worker effects | Host-facing intents, single-use launch preparation and bound observations; the cooperating host performs invocation. R16 deterministic/recovery evidence is not a fresh live-model invocation. |

Two ordinary process semantics are now explicit in the public CLI guide.
Material paths are based on the supplied configuration directory; other
application paths are used as supplied, so relative values depend on launch
cwd. The configured Vibe child inherits the host environment and cwd while
receiving an explicit project path and offline/agent arguments. No hermetic
subprocess or credential-environment sanitization guarantee is claimed.

Disposition: correct the inaccurate audit-policy debt and retain the actual
effect boundary and inherited-process semantics in public usage documentation.
No conformance baseline, gate or exemption is widened. There is no missing
FS/process/network registry in ZAP to implement under the existing tool
contract. A future generic effects analyzer would be a separate toolchain
feature, not evidence fabricated for this release. Final stable-source
conformance and installed-adapter checks remain required in their own gates.
