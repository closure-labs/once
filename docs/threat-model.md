# Threat model

## Trusted claims

A build-trace signature is a claim by a configured builder. Legacy local
policies rely on Nix’s trusted local store and then apply a narrower accepted-key
policy. A signer name in JSON is never sufficient by itself. Portable bundles in v0.5
are verified directly by Once using full Ed25519 public keys from protected
policy. Verification covers Nix’s exact signed derivation/output mapping before
the claim enters the temporary cache. This path is restricted to the tested
Nix 2.35.3 format.

Nix 2.35's remote build-trace inspection command does not perform that
validation. Remote trace hits consequently fail closed as `UNSUPPORTED` in
v0.2. Locally registered traces rely on the local store as the trust boundary.
The audited Nix 2.36 prerelease validates realisation signatures inside its private
substitution machinery, but exposes neither a read-only validating CLI nor a
stable public build-trace C API. Once does not bind to that private ABI or begin
substitution merely to establish trust; see the
[remote trust audit](planning/remote-trust-audit.md).

Compromise of an accepted signing key permits false build-trace claims until the
key is revoked. Private keys must not be committed, exposed to untrusted pull
requests, or placed in public test fixtures.

## Candidate-controlled policy

A malicious change can weaken a check or alter its policy version. Repository
pull requests, required CI, and protected workflow/configuration files are the
v0.1 boundary. CODEOWNERS records responsibility, but approving owner review
cannot be required until Closure Labs adds a second maintainer.

In v0.2, protected CI can instead load policy from a separately protected flake
pinned by a full commit and verified against Nix metadata. This prevents a
candidate from changing policy contents through `.once.toml`. The protected
workflow must also own the flake reference and expected revision; allowing a
candidate to choose either value would move the vulnerability rather than
remove it. Candidate derivation semantics still require repository review.

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


## Portable bundles and GitHub artifacts

The bundle contains native claims, not a trusted store database. Neither its
artifact name nor unsigned metadata binds it to the current target. The current
derivation is evaluated independently with IFD disabled and no lock-file
updates. Nix resolves it using verified claims and the current evaluated
derivation metadata. Staging this metadata never restores target outputs.
Missing evidence cannot be replaced by the producer's store database. Unknown
formats, invalid signatures, and conflicting mappings fail closed.

Bundle transport uses successful push runs of a configured producer workflow
on a protected branch. Consumer jobs receive read-only permissions and no
signing credentials. Fork jobs without artifact access use the documented
ordinary-check fallback. ZIP paths, sizes, and symlinks are checked before
extraction; only native signature verification authorizes a skip.

`policyVersion` must be embedded in the check derivation and match protected
policy's `once.policy_version`. Bumping it invalidates computation identity.
Changing accepted keys changes acceptance immediately; changing the policy
flake commit alone does not force a rebuild. Artifact retention limits reuse,
not signature lifetime: an authorized old claim remains valid for the same
derivation until policy revokes its key or changes computation identity.
