# Diagnostic commands

Once v0.2 gives each read-only diagnostic command a focused view while keeping
one policy evaluation and one set of stable decision exit codes underneath.
None of these commands builds the requested installable; each may materialize
the separately pinned policy text object.

`once resolve <installable>` reports the unresolved derivation from evaluation
and the resolved derivation/output identity exposed by the native base trace.
Its JSON schema is `dev.closurelabs.once/resolve/v1`.

`once trace <installable>` reports native trace entries, output paths, opaque
paths, signer-key metadata, signature counts, and whether the evidence came
from the trusted local-store boundary or an unverified remote query. It does
not independently claim that signature metadata is valid. Its JSON schema is
`dev.closurelabs.once/trace/v1`.

`once trust <installable>` projects the configured signature and IA policy,
accepted signers, decision, action, and whether policy permits a skip. Its JSON
schema is `dev.closurelabs.once/trust/v1`.

`once explain <installable>` gives the expanded human-readable evaluation,
resolution, input, trace, and decision narrative. With `--json`, it deliberately
retains the stable `dev.closurelabs.once/result/v1` envelope used by `check` and
`run`, so existing result consumers do not need another compatibility path.

The diagnostic JSON projections retain `decision` and `action`, and the
process exit code remains the same code returned by `once check` for the same
state. A trace view can therefore display signer metadata while still returning
`UNTRUSTED` or `UNSUPPORTED`; its native trace status can remain `found` while
the separate policy decision fails. Visibility is not acceptance.
