# Remote build-trace trust audit

**Audit date:** 2026-08-23
**Scope:** Once v0.2.0 and follow-up version proof
**Decision:** Remote build-trace reuse remains `UNSUPPORTED`.

## Question

Can Once verify a remote build-trace claim cryptographically, bind it to the
resolved derivation/output key, and identify an accepted signer without
building, substituting, copying, or otherwise realizing the referenced output?

## Audited revisions

- Nix 2.35.2 at
  `2c73b59da29606068c0c98db015dd3a66955525d`, the version targeted by
  Once v0.2.
- Nix upstream at
  `88b09c64fbea076a0376830d98e5331f70ed31a3`, reporting
  `2.36.0pre20260822_88b09c6` when this audit was performed.

The audit covered the build-trace CLI implementation, realisation signature
validation, substitution goals, and the public C store API.

Both Nix repositories were downloaded as full-revision-pinned, immutable local
source trees with `nix flake archive`. All `libstore` and CLI source findings
were checked from those local copies. The reviewed files and reproduction
commands are recorded in the
[2.35.2/2.36 proof](nix-2.35-2.36-build-trace-proof.md#local-source-review).

## Findings

### Nix 2.35.2 CLI

`nix store build-trace info` converts the requested path to realised paths and
serializes each `Realisation`. It does not call `checkSignatures` or
`realisationIsUntrusted`. A remote response therefore exposes signature
metadata but does not prove that any signature is valid or trusted.

In Nix 2.35.2, derivation-output substitution accepts a remote realisation
without validating its signature first. Reusing that internal path would also
cross Once's read-only boundary by beginning substitution of the output.

`nix store verify` is not an alternative. It validates store-path/NAR metadata,
not the signed realisation that maps a resolved derivation output to an opaque
output path, and content verification reads the referenced NAR.

### Current Nix upstream

Upstream has improved its internal substitution machinery:

- `018d6462def78e6f1b940d497ffaa3828831af03` separated build-trace
  retrieval from store-object fetching, although its normal caller proceeds to
  path substitution.
- `1a17ffbb557dd797c5df995a34a43b59a395f3d4` made
  `DrvOutputSubstitutionGoal` reject a remote realisation when
  `realisationIsUntrusted` finds no signature from the store's trusted public
  keys.

These are private C++ libstore paths used during substitution. The read-only
`build-trace info` command still serializes remote realisations without calling
the validation primitive.

The isolated version matrix confirms both behaviors. With an unrelated key,
Nix 2.35.2 realizes the content-addressed output, while the pinned 2.36
prerelease rejects the build trace and leaves the output absent. In both
versions, the read-only command returns identical trace JSON for the accepted
and unrelated keys without requesting the referenced output NAR. Once returns
`UNSUPPORTED` in both cases.

### Public C API

The public store API exposes store-path operations and a `checkSigs` option
when copying a path. It does not expose a keyed `Realisation`, build-trace
query, detached-realisation signature check, or `realisationIsUntrusted`.

Once could link against private C++ libstore internals, but that would add an
unstable ABI dependency and still require careful policy binding for validated
key identity and signature thresholds. That is not an acceptable v0.2 trust
boundary.

## v0.2 decision

No audited public Nix interface satisfies all of Once's requirements. Once will
continue to accept reuse only when the trace is registered in the trusted local
store. A remote trace hit fails closed as `UNSUPPORTED`; signer labels and raw
signature strings are never treated as cryptographic evidence.

This is a completed feasibility result for v0.2, not an unimplemented remote
backend.

## Reopening criteria

Remote reuse may be reconsidered when Nix provides either:

1. a read-only CLI that rejects invalid realisation signatures, reports which
   configured key identities met the threshold, and does not realize the
   referenced output; or
2. a stable public API for querying a keyed realisation and validating its
   exact signatures against explicit public keys.

An integration test must then prove that the exact resolved derivation/output
key succeeds with the intended key, fails with an unrelated key, fails on
conflicting output paths, enforces configured key identity and threshold, and
leaves the referenced output absent before and after the check. The invocation
log must contain no target build, substitute, copy, path-info, or NAR request.
