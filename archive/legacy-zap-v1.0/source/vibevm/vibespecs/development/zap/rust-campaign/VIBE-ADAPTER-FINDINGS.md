# Verified installed VibeVM adapter surface

Read-only coordinator probes, 2026-09-13. No source/lock mutation, model call,
NEXT activation or publication was performed.

The PATH-installed `vibe` and the host-development `target/debug/vibe.exe`
both report version1.0.0 but have different capabilities and lock schemas.
The installed tool created the package's schema6 vibe.lock. The newer host
binary expects schema7 and its requirements query refuses the schema6 lock.
Do not rewrite a consumer lockfile merely to force an adapter probe, and do not
negotiate capability from the display version alone.

The newer binary has `requirements --address-prefix --limit --relations --json`
and `--agent-mode agent`; installed PATH vibe lacks both requirements and
agent-mode. These are observed differences, not assumptions about every host.

The common installed read-only query surface WORKS against the current ZAP
package without any host source dependency:

```text
vibe query --path <project-root> --uri spec://org.vibevm.world/zap/flows/zap/ZAP-RUNTIME#PORTABLE-RUNTIME --limit 2 --json --offline
```

Observed response: count1, total_matching1, truncated=false; one `source=spec`
row has the exact URI, normalized normative text with authoring status,
relative file `vibevm/vibespecs/flows/zap/ZAP-RUNTIME.xml` and line7. This is a
real portable metadata/source-location seam for R15. Its status is a declared
fact, not verification or consumer acceptance.

An adapter may combine this actual bounded query with a contained source-file
capture and exact byte digest. Validate query/source identity and scope before
recording a trusted capture, handle source drift, and keep normative metadata
separate from observations/evidence. Do not fork an entire fact engine or claim
that metadata alone captures source bytes.

Queries have explicit count/total/truncation. A truncated answer is incomplete;
use narrower known addresses/partitions or a supported fuller metadata source.
Never treat the first bounded page as the whole project. Probe actual command
and option support once per executable/configuration identity, and preserve
unsupported diagnostics. The read-only query does not invoke inference even
when the older tool lacks agent-mode.

Use an injected/discovered executable in the product. Absolute paths used in
this machine's probes are not production defaults. R13 owns public exposure;
R15 owns the real capture/native-fact adapter and exact promotion effect.
