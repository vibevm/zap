# Root review of core, runtime, store and app usage bindings

Accepted bounded I/J documentation units, 2026-09-14. Root read the complete
four guides, reviewed their constructor/acquisition distinctions and exact
symbol catalogs, and inspected the mapping receipts and reported scoped gates.
Final package-wide determinism remains a separate writer-freeze gate.

I covers195 source-level declarations:109 runtime,23 store and63 app. Its189
unpinned documents edges and6 retained compiled examples have exact guide
targets. Publicly exposed store associated handles and private implementation
declarations are distinguished from named public imports. J covers313 core
types:306 frontend-visible declarations plus7 macro emissions. Forty-seven
compiled examples remain;259 explicit edges and3 generator attributes document
the remaining266 types. Generated names belong to their actual use groups.

Root independently compared the complete source directories against the09:20
capture while ignoring line-ending-only differences. Outside the already
accepted raw-page test and counter-deserialization units, changes consist of
spec metadata, imports and formatting. The only structural I extraction moves
six unchanged private store metadata constants into engine/metadata.rs and
imports them back into the same parent scope. Values and external visibility
are preserved; the engine is598lines. No other behavior change was hidden in
these documentation units.

Every selected crate reports zero actual public-type findings and its gate is
enabled without baseline widening or exemption. Check, strict scoped lint and
formatting passed. Core ran47 doctests (45 runnable,2 compile-fail), runtime3,
app3 and store0; store coverage is documentary. The separate positive-version
deserialization correction has its own accepted receipt and regression.

The combined generation reported582 spec units,955 tagged items,1030 edges,
zero warnings and zero suspects. The three remaining domain viewer-index helper
orphans are explicitly assigned to R17-RUNTIME-SCALE. The subsequent --check
correctly detected the concurrently growing domain guide; neither worker nor
root counts this as a final deterministic package index. Root will regenerate
after domain/runtime metadata and the canonical-byte preparation are complete.

The active runtime continuation repair is additional product work. These
documentation receipts do not claim that the final runtime source or installed
package has already passed its release gate.
