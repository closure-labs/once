# Nix 2.35/2.36 build-trace signature proof

**Run date:** 2026-08-23
**Status:** Reproducible proof of concept
**Once decision:** Remote trace-only acceptance remains `UNSUPPORTED`.

## Result

The same unresolved content-addressed derivation was tested against a signed
HTTP binary cache with either its accepted public key or an unrelated key.

| Observation | Nix 2.35.2 | Nix 2.36 prerelease |
| --- | --- | --- |
| Read-only query returns trace with accepted key | Yes | Yes |
| Read-only query returns identical trace with unrelated key | Yes | Yes |
| Read-only query downloads referenced output NAR | No | No |
| Once accepts the remote trace | No: `UNSUPPORTED` | No: `UNSUPPORTED` |
| Unrelated-key substitution is rejected | No | Yes |
| Unrelated-key substitution realizes the output | Yes | No |
| Accepted-key substitution realizes the output | Yes | Yes |

The prerelease improvement is real but applies inside substitution. The
read-only `nix store build-trace info` interface still returns unverified
realisation data, so it does not provide Once with cryptographic authorization
to skip a build.

## Exact inputs

- Baseline package:
  `github:NixOS/nixpkgs/2c423e03bbafcff28bfadc6781a4a8257f205cb5#nixVersions.latest`,
  resolving to Nix 2.35.2.
- Baseline Nix source: tag `2.35.2`, commit
  `2c73b59da29606068c0c98db015dd3a66955525d`.
- Forward package and source:
  `github:NixOS/nix/88b09c64fbea076a0376830d98e5331f70ed31a3`,
  reporting `2.36.0pre20260822_88b09c6`.

The matrix pins full revisions; it does not depend on a moving branch,
FlakeHub resolution, or the host's installed Nix version for either case.

## Run it

```console
$ nix develop
$ cargo build
$ ./scripts/build-trace-version-matrix.sh | jq .
```

The matrix runs sequentially because the forward Nix package and disposable
store closures are comparatively large. It emits one versioned JSON document
and exits nonzero if either pinned behavior changes.

## Isolation and security controls

- All producer, evaluator, consumer, and cache stores are created below a
  mode-0700 temporary directory and removed on exit.
- Secret keys are generated for the run, protected by `umask 077`, never
  printed, and never written into the checkout or host Nix configuration.
- The accepted and unrelated keys are generated independently for every case.
- Consumer stores explicitly set `require-sigs=true`.
- Cache publication uses a local writable store, but reads use an unprivileged
  HTTP server bound only to `127.0.0.1`. A `file://` reader is not a valid
  negative-signature control because local cache access can take a trusted
  path through Nix.
- Evaluation receives only an opaque derivation closure. It does not inherit
  the producer's realisation database, ensuring the build goes through remote
  derivation-output trace substitution.
- The harness asserts that read-only inspection never requests the referenced
  output NAR before either substitution attempt.
- No long-lived daemon setting, trusted key, network listener, or repository
  credential is changed.

The loopback server deliberately uses plain HTTP because transport secrecy is
not under test and no persistent secret is served. It must never be rebound to
a non-loopback address. The signature checks, full revision pins, isolated
stores, and negative key are the relevant controls for this experiment.

## Local source review

The `libstore` conclusions were checked from immutable local source trees, not
from web-rendered source. Recreate those trees with:

```console
$ nix flake archive --json github:NixOS/nix/2.35.2 | jq -r .path
$ nix flake archive --json github:NixOS/nix/88b09c64fbea076a0376830d98e5331f70ed31a3 | jq -r .path
```

The comparison covered:

- `src/libstore/build/drv-output-substitution-goal.cc`;
- `src/libstore/local-store.cc` and `src/libstore/realisation.cc`;
- `src/nix/build-trace.cc`;
- the public C store API under `src/libstore-c`.

The forward tree adds a trust check in `DrvOutputSubstitutionGoal`: an
untrusted substituter's realisation is ignored when
`realisationIsUntrusted` cannot validate it against configured trusted public
keys. Nix 2.35.2 lacks that check. Neither locally inspected CLI implementation
validates realisation signatures before serializing the read-only
`build-trace info` response.

## Limits

- `2.36.0pre20260822_88b09c6` is prerelease software and is research-only.
- The new enforcement path is private C++ `libstore` behavior, not a stable
  public Once integration surface.
- A valid content-addressed NAR proves content integrity, not who authorized
  the derivation-output mapping. That is why the 2.35.2 unrelated-key result is
  meaningful even though the copied output itself remains content-addressed.
- This proof does not enable remote skipping. It supplies regression evidence
  and a concrete upstream capability to track.
