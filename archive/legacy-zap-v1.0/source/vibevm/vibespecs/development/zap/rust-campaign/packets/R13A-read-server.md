# R13A: finish the independent local read server

##subagent-quiet-clause

Middle gpt-5.6-sol/high. This is a bounded execution slice of the already
authorized R13 packet, not a new planning or approval task. Read R13-surface.md
and reuse its exact standing rules and currently saved source. Own zap-api,
zap-app and zap-cli. Do not wait for R07: this slice reads the existing actual
ReadApplication and invokes no mutation, economics or dispatch provider.

Implement and run a real local HTTP server exposed by the existing zap binary.
Use the closed typed machine request/response and actual snapshot, registered
query and event-tail reads already implemented. Follow the packet's existing
backend protocol where compatible. Use protected read configuration and
configured source/campaign roots; do not accept arbitrary filesystem paths,
actor labels or credentials in public payloads. Default binding is loopback.
Never print credentials or expose them through command-line values.

Provide actual snapshot and resumable event-tail streaming, bounded buffering,
cancellation and explicit stale/resync/backpressure outcomes. A slow or
disconnected client must not retain a writer transaction or block other reads.
Expose only the available read operations; pending mutations remain explicitly
unavailable until their own slice is implemented. Do not simulate successful
commands or native operations.

Acceptance: start the compiled binary against an isolated real redb store,
perform actual HTTP snapshot/query and event-tail requests, reconnect from a
saved cursor, verify foreign/stale cursor refusal and client cancellation,
and demonstrate a slow reader cannot freeze another read. Use one coherent
integration scenario and scoped static checks, no host test panel or model call.
No NEXT store, external launcher or local inference.

Save implementation, exact run evidence and checkpoint after each coherent unit
and before long commands, within five minutes active work. Continue until this
bounded server slice is complete or a concrete external failure prevents it.
An R07 signature wait cannot block this slice. Then return the bounded result;
root assigns the next R13 slice and still owns complete R13 acceptance.
