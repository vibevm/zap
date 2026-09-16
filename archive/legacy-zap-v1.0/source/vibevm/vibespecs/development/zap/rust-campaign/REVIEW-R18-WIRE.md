# Canonical wire usage review

Root accepts the bounded zap-wire public-use batch in REPORT-R18-F.md.
The remaining public-type, test-structure and package gates stay separate.

Compared the wire source with the retained pre-batch source capture. Production
differences are documentation and inert metadata; the only test-body change
replaces panic helpers while retaining both invalid-value/unsupported-codec
precedence assertions. Public signatures, visibility, encoding and runtime
logic are unchanged.

The co-located examples use public constructors and demonstrate canonical
encoding, exact identity, counters, typed subjects, command construction and
structured refusal. Macro examples expand with the public type family rather
than leaving generated exports invisible. The docs distinguish syntactic
construction from registered support and authority; digest wrapper examples
do not invent a domain salt.

Reported evidence is 131 executed Rust doctests, eight wire contracts, the
focused precedence case, strict all-target lint and scoped conformance. The
wire public-type gate is enabled only after the actual zero-gap result. This
does not claim the broader 962-declaration documentation inventory is closed.
