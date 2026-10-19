# Architecture

Once has three layers. The Nix adapter invokes upstream Nix using argument
arrays and normalizes its JSON at one boundary. The policy engine classifies the
normalized result. The reporter renders the same decision for people, JSON
consumers, and GitHub Actions.

Nix owns evaluation, unresolved-to-resolved derivation resolution, build-trace
storage, and local-store signature validation. For portable bundles, Once
verifies native Ed25519 claims before letting Nix resolve the current target.
Once does not maintain a parallel derivation resolver or hash graph.

In Nix 2.35, `build-trace info` does not validate signatures returned by a
remote store. Direct remote trace hits therefore return `UNSUPPORTED`. v0.5 adds a separate
verified-bundle boundary using public keys supplied by immutable policy. The v0.2
[upstream audit](planning/remote-trust-audit.md) found no read-only CLI or
public C API that closes this gap. Once does not substitute signer-name
matching for cryptographic verification.

The signature threshold remains exactly one. Higher thresholds are deferred.
The portable verifier reports only cryptographically verified accepted keys.
Bundles contain native base claims. The consumer stages current evaluated
derivation metadata in a disposable native cache alongside verified claims;
target output artifacts and producer store databases are never imported.

`lib.mkOnceCheck` creates a tiny content-addressed derivation that depends on a
target derivation and embeds the Once policy version in its semantics. A target
or policy change therefore changes the resolved check identity even when the
small result payload remains byte-for-byte identical.

For protected CI, v0.2 can materialize configuration from an external policy
flake pinned by a full GitHub commit. Once independently requires the Nix
metadata revision to match the expected commit, realizes only the flake's tiny
`#policy` text output, and reads it through the configured Nix store. Mutable
policy references fail before target evaluation. Local TOML remains available
for development but is not an authoritative protected-CI boundary.


Portable reuse requires a floating CA check on x86_64-linux with `policyVersion`
matching configured `once.policy_version`. The native format is pinned to Nix
2.35.3 until additional versions pass compatibility and fresh-store tests.
`once export` produces a trace-only directory; `once check --trace-bundle`
verifies it and asks Nix to resolve the current target. `once bundle-key` emits
an artifact lookup name, which carries no independent trust.
