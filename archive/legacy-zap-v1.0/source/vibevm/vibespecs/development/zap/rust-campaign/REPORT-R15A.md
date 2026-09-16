# R15A production material, workspace and portable archive adapters

Status: review candidate. This report covers the bounded R15A adapter atom. R13C owns the application service and transport composition that consumes these ports.

## Delivered surface

`zap_app::material_adapters` now supplies the coordinated production types:

- `FilesystemMaterialAdapterConfig`, protected root/material/workspace bindings and positive archive/query limits;
- `build_filesystem_material_adapters(config_dir, config)` and `FilesystemMaterialAdapters`, exposing the four `Arc` ports required by R13C;
- `FilesystemPacketMaterialProvider`, `FilesystemPacketWorkspaceProvider`, `PortableBundleArtifactProvider`, and `ImmutableArtifactRepository`;
- `PublishedPortableBundle`, `VerifiedPortableBundle`, `PortableBundleEntry`, and typed `PortableBundleEntryBody` inspection for a neutral consumer.

Relative paths resolve against the protected configuration directory. Material capture requires an exact configured StoreId, BaseId, Revision and source/rule/fork subject. The adapter validates the ordinary-file path under an approved canonical root, rejects traversal, sensitive credential/configuration names and symlink/reparse ancestors, reads within a positive byte bound, verifies the source or semantic digest, then publishes the captured bytes through `zap_store::ArtifactStore`.

Historical verification opens the content-addressed artifact and checks its exact byte length and SHA-256 identity. It never substitutes a current source capture. R13C's bundle preparation locates the RuntimeJobRecord insertion through indexed record history, reads that exact committed event revision and recovers its schema-2 `RuntimeJobClaimRecord`; source, rule, fork and workspace artifact checks use that immutable claim.

Native VibeVM specification bindings use a configured executable; there is no shipped machine path or version-only assumption. Construction checks the actual `vibe query --help` capability for `--path`, `--uri`, `--limit`, `--json`, `--offline`, `--unattended`, `--invoked-by` and `--agent-mode`. Capture invokes an exact URI query with limit 2, offline, unattended and explicit agent mode, requiring one untruncated `source = spec` result whose URI and relative file equal the protected binding. A bounded process reader enforces the combined stdout/stderr byte ceiling while reading, kills on overflow or configured timeout, reaps the child, and does not include child stderr in diagnostics.

Workspace capture emits canonical `zap-app/packet-workspace-manifest/1` JSON. It binds the exact store/base/packet/lowering/work/harness and workspace identity, approved root IDs and access, declared read/create/replace/delete operations, instruction isolation, checks and outputs. Mutations require a read-write root; outputs must be declared create or replace operations. The manifest explicitly records `provides_os_sandbox: false`.

## Portable archive contract

The implementation is split into a 397-line publication/API module, a 201-line binary codec module and a 503-line semantic validation module.

The archive container is version 2 (`ZAPBNDL2`). It contains the exact canonical R09 `WeakBundleManifest`, a bounded ordered entry count, every entry's kind/path/artifact/length and bytes, plus the manifest and entry-set digests. App-derived semantic entries use `ZAPENTRY2` frames carrying the exact canonical typed body and its digest:

- Packet: full `WorkerPacketRecord` plus the committed `RuntimeJobClaimRecord`;
- Assignment: full executable `TaskContractRecord` plus `RuntimeJobRecord`;
- ResultSchema: exact `CandidateResultContract`;
- Capability, Permission and StopRule: their exact typed records/bindings;
- Source, Rule, Fork and Workspace: the immutable raw captured bytes.

Before publication the adapter reseals the manifest, independently derives the required entry destinations from its packets, attempts, sources, rules, forks, capabilities, permissions and stop rules, and rejects missing, extra, duplicate or conflicting paths. It verifies every artifact byte identity. Semantic bodies are strictly decoded and cross-linked back to the manifest's strategy, lowering, packet, contract, job, attempt, producer, basis, capability, workspace and active stop-rule identities. Contract digest is rederived from the full TaskContract body. Thus a neutral consumer can read the instructions, checks, acceptance, result contract and raw rules from `VerifiedPortableBundle` without this checkout, original database or artifact store.

Encoding checks the total budget before every buffer extension, including framing and footer. Reopen uses one ordinary non-reparse file handle, reads through `take(maximum + 1)`, and rejects growth, truncation, trailing bytes, unknown versions and every length/hash mismatch.

Publication first uses the existing recoverable `ArtifactStore` content-addressed protocol, then atomically hard-links that verified object to the protected archive destination. An exact existing destination is reverified; any foreign/partial destination is preserved and refused. `ArtifactStore::open_verified(digest, expected_len)` and `PublishedArtifact::read_verified()` are the narrow additive restart-safe read seam; focused store tests cover restart reopen, wrong length, same-length hash corruption and a non-file content path.

## Evidence

All Cargo commands used the verified `run-cargo.ps1` wrapper and exact `C:/Users/olegc/.vibe/zap/build/next-rust` target.

- `test -p zap-store artifact::tests`: 3 passed.
- `test -p zap-app --lib material_adapters`: bounded-process overflow, ordinary hang and closed-stdout/stderr hang kill-reap cases plus the Windows junction/reparse escape test passed. Process completion remains under the same deadline after both streams reach EOF; cancellation drops readers before reap and never joins a pipe reader that may be retained by a descendant.
- `test -p zap-app --test material_adapters`: 3 passed; actual file source/rule capture is content-deduplicated across two packet requests, two exact workspace manifests remain distinct, immutable historical verification survives current-source drift, recapture refuses drift, and traversal refuses.
- The native-spec test was rerun with `ZAP_R15A_VIBE_EXE` and `ZAP_R15A_VIBE_PROJECT` supplied by the host fixture: 1 passed using the exact offline/unattended/invoked-by/agent-mode query. Neither path is compiled into or defaulted by the product.
- `test -p zap-app --test packet_resolution_service real_lowered_packet_seals_runtime_claim_and_replays_captured_material -- --exact`: 1 passed in 3.79s after the final source split. This real service chain uses production filesystem material/workspace/bundle providers, current-source drift after claim, the indexed committed claim and immutable artifact verification, typed semantic artifacts, archive publication, a copied neutral-file reopen with Packet/Assignment/ResultSchema/raw Rule inspection, trusted Ready receipt, and foreign partial destination preservation.
- `clippy -p zap-store --lib --no-deps -- -D warnings`: passed.
- `clippy -p zap-app --all-targets --no-deps -- -D warnings`: passed.

## Boundaries and remaining work

This manifest describes allowed workspace roots and operations; it does not create or claim an operating-system sandbox. The archive is uncompressed by design in this atom and all sizes remain explicitly bounded. The focused service journey has one packet; the independent production adapter fixture covers the requested two-packet overlapping source/rule publication behavior without inventing a second semantic graph.

R15B owns package/install/CLI documentation and neutral installed-consumer proof. R13C owns final closed application configuration, service factory, transport and CLI wiring. No NEXT source/store/pointer, model/local inference, Git, publication or full host panel was touched.

Two previously denied build-directory cleanup debts remain under the root's no-retry instruction, including `C:/Users/olegc/.vibe/zap/build/next-r Dee`; this report does not claim cleanup completion.
