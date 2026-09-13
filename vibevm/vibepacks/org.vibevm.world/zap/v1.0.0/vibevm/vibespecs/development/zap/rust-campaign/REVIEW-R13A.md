# Root review of the first real read server

Status: bounded R13A repair required. The actual binary/server receipts support
the normal read/cursor path, but do not cover these source-level edge failures.

1. `server.rs` breaks body reading on EOF and then slices to Content-Length;
   truncated input panics. Adding header length to an attacker-controlled length
   may overflow. Validate framing with checked arithmetic and require the exact
   body before slicing or decoding.
2. Connection capacity is released only after normal handler return. A panic
   can leak a slot. Use a lifetime guard; verify recovery after malformed input.
3. No socket write timeout is set, including the 429 response on the accept
   thread. A nonreading client can retain a slot or block acceptance/shutdown.
   Bound writes, cancellation and shutdown/drain behavior.
4. HTTP header names are case-insensitive. Normalize them, refuse conflicting
   body-length framing, and match exact paths instead of arbitrary prefix
   suffixes. An established HTTP server/parser is preferred to maintaining
   custom protocol framing; dependency additions are authorized routine work.
5. Finite SSE currently serializes an event page into an unbounded byte Vec.
   A record-count bound is not a byte bound. Provide configurable bounded
   serialization/response handling or actual bounded streaming, with explicit
   oversize/resync state and no silent complete/truncation claim. This does not
   create a universal campaign or boot-token cap.

Extend the existing real server scenario with the discriminating cases above;
reuse normal-path receipts where source remains applicable. The worker is
authorized to use a maintained framework such as axum/hyper with suitable
Tokio/tower limits. Full R13 protected mutation and named graph-query work
remains separate and unfinished.
