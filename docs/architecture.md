# Architecture

Once has three layers. The Nix adapter invokes upstream Nix using argument
arrays and normalizes its JSON at one boundary. The policy engine classifies the
normalized result. The reporter renders the same decision for people, JSON
consumers, and GitHub Actions.

Nix owns evaluation, unresolved-to-resolved derivation resolution, build-trace
storage, and signature validation. Once does not maintain a parallel receipt or
hash graph. Backend support only selects/configures a Nix store.

In Nix 2.35, `build-trace info` does not validate signatures returned by a
remote store. Once therefore treats the local store as the only supported trust
boundary for v0.1 and returns `UNSUPPORTED` for remote trace hits. It does not
substitute signer-name matching for cryptographic verification.

The v0.1 signature threshold is exactly one. Higher thresholds remain
unsupported because the Nix 2.35 public CLI cannot prove which individual
signatures passed validation.

`lib.mkOnceCheck` creates a tiny content-addressed derivation that depends on a
target derivation and embeds the Once policy version in its semantics. A target
or policy change therefore changes the resolved check identity even when the
small result payload remains byte-for-byte identical.
