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
boundary and returns `UNSUPPORTED` for remote trace hits. The v0.2
[upstream audit](planning/remote-trust-audit.md) found no read-only CLI or
public C API that closes this gap. Once does not substitute signer-name
matching for cryptographic verification.

The signature threshold remains exactly one in v0.2. Higher thresholds remain
unsupported because the Nix 2.35 public CLI cannot prove which individual
signatures passed validation.

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
