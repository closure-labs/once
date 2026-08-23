# Threat model

## Trusted claims

A build-trace signature is a claim by a configured builder. Once delegates
cryptographic validation to Nix and then applies its narrower accepted-key
policy. A signer name in JSON is never sufficient by itself.

Nix 2.35's remote build-trace inspection command does not perform that
validation. Remote trace hits consequently fail closed as `UNSUPPORTED` in
v0.1. Locally registered traces rely on the local store as the trust boundary.

Compromise of an accepted signing key permits false build-trace claims until the
key is revoked. Private keys must not be committed, exposed to untrusted pull
requests, or placed in public test fixtures.

## Candidate-controlled policy

A malicious change can weaken a check or alter its policy version. Repository
review and protected workflow/configuration files are the v0.1 boundary. A
future release should source policy from a separately protected flake or
repository that candidate code cannot redefine.

## Nix boundaries

Input-addressed dependencies provide weaker direct content identity than
content-addressed dependencies. Unknown dependency classification fails closed
in `deny` mode and is disclosed in `warn` mode.

Nondeterministic derivations can produce incompatible realizations. Once rejects
multiple output paths for one resolved derivation/output key as `CONFLICT`.
Impure derivations, sandbox differences, platform differences, and Nix-version
differences can all invalidate assumptions about equivalence.

A cache can replay a valid old trace only for the same resolved derivation key.
Changing a build-relevant input or policy changes that key. Cache compromise
without an accepted signing key should be rejected by Nix; cache plus signing
key compromise is equivalent to a trusted-builder compromise.

Ultimately, independent audit still requires rebuilding. Once is memoized trust,
not trustless proof of computation.
