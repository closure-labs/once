# xo-nixpkg integration example

This example is based on the local `xo-nixpkg` v0.9.5 repository. That project
already pins three Xen Orchestra source channels, builds `latest`, `stable`,
and `rolling` packages, and emits matching supply-protector outputs containing
the runtime closure graph, an assertion, checksums, and SPDX and CycloneDX
documents.

Once complements those controls by recognizing the result of one exact,
content-addressed verification derivation. It does not replace the package,
the supply-protector documents, normal Nix substitution, or a rebuild used for
independent reproducibility testing.

## The check

[`examples/xo-nixpkg/flake.nix`](../../examples/xo-nixpkg/flake.nix) is a
standalone consumer flake pinned to Once v0.4.2 and xo-nixpkg v0.9.5. Its
`xo-latest-once` check uses `once.lib.mkOnceCheck` to:

1. depend on the exact `latest` supply-protector output and, transitively, the
   Xen Orchestra package it describes;
2. require the packaged `xo-server` executable;
3. verify every digest in the supply-protector `SHA256SUMS` file;
4. require the assertion subject to equal the selected Xen Orchestra store
   path and the asserted distribution cache to equal the xo-nixpkg cache; and
5. include `xo-nixpkg-supply-v1` in the check derivation's semantics.

The result is deliberately tiny. The expensive package and its supply
documents remain normal Nix outputs, while the content-addressed Once check is
the recognition key.

During validation in a clean isolated store, this locked example planned 154
substituted paths totaling 662.5 MiB compressed and 2.2 GiB unpacked, then
built five remaining derivations: three pinned document schemas, the
supply-protector output, and the tiny Once check. Those figures are one
x86_64-linux observation rather than a stable size contract, but they show why
future trace-only recognition would matter for this repository.

Evaluate or build the example from this repository:

On a multi-user installation, the Nix daemon must have `ca-derivations`
enabled before the build. Passing the feature on the client command line does
not add it to an already-running daemon.

```console
$ nix flake show --accept-flake-config ./examples/xo-nixpkg
$ nix build --accept-flake-config \
    --extra-experimental-features ca-derivations \
    ./examples/xo-nixpkg#checks.x86_64-linux.xo-latest-once
$ jq . result/result.json
```

The example also packages a policy-bound runner:

```console
$ nix run --accept-flake-config ./examples/xo-nixpkg#once -- doctor
$ nix run --accept-flake-config ./examples/xo-nixpkg#once -- \
    explain ./examples/xo-nixpkg#checks.x86_64-linux.xo-latest-once
```

The checked-in `once.toml` is appropriate for this local example. A protected
workflow should instead invoke Once with a policy flake pinned by a full commit
so a candidate cannot weaken its own acceptance policy.

An unsigned developer store is expected to fail closed. After building the
example in an unsigned isolated store, `once explain` reports `UNTRUSTED`, zero
accepted signers, and no skip action. This is distinct from a cold `MISS` and
confirms that merely finding a local trace is insufficient.

## Signed producer setup

Once accepts a local trace only after Nix has registered it through a trusted
store boundary. The example policy expects a dedicated build-trace key named
`xo-once-ci-1`. That is not the same credential as the
`xen-orchestra-ce.cachix.org-1` public binary-cache key or a Cachix auth token.

For a persistent trusted producer, generate the key outside the checkout and
configure the producer's Nix daemon with its private key:

```console
$ test ! -e /secure/path/xo-once-ci-1.private
$ umask 077
$ nix key generate-secret --key-name xo-once-ci-1 \
    > /secure/path/xo-once-ci-1.private
$ nix key convert-secret-to-public \
    < /secure/path/xo-once-ci-1.private \
    > /secure/path/xo-once-ci-1.public
```

The administrator-managed Nix configuration must enable `ca-derivations`, set
`secret-key-files` to the private key on the producer only, and distribute the
public key to verifiers through a separately protected configuration. Do not
commit the private key, expose it to pull-request jobs, or treat a Cachix token
as a build-trace signing key.

With that boundary in place, the first protected invocation is a miss and may
build the check:

```console
$ nix run --accept-flake-config ./examples/xo-nixpkg#once -- \
    run ./examples/xo-nixpkg#checks.x86_64-linux.xo-latest-once
```

A later invocation against the same trusted local store can recognize the
registered trace without rebuilding the requested installable:

```console
$ nix run --accept-flake-config ./examples/xo-nixpkg#once -- \
    check ./examples/xo-nixpkg#checks.x86_64-linux.xo-latest-once
```

For v0.4.2, the expected successful decision is
`ACCEPTED_WITH_IA_TRUST`, not `ACCEPTED_CA`, because xo-nixpkg's package and
supply-protector derivations are input-addressed and Nix 2.35 does not expose
the transitive classification needed for a stronger decision.

## Benefit now and later

| Capability | Once v0.4.2 with xo-nixpkg | Potential with mature CA validation |
| --- | --- | --- |
| Exact invalidation | The check identity changes when the selected XO package, supply-protector logic, or policy version changes. Unrelated repository documentation can leave it unchanged. | The same rule remains, with richer diagnostics identifying which content-addressed input or policy edge invalidated recognition. |
| Trusted recognition | A protected producer can reuse a Nix-validated trace already registered in its local store. | A clean runner could validate a remote trace from Cachix, S3, or Harmonia before accepting it. |
| Remote CI savings | None from trace-only recognition. A remote trace is `UNSUPPORTED`; ordinary Cachix substitution can still download the built output. | A pull request whose resolved CA check already has an authorized trace could skip the expensive XO realization and supply-document regeneration without downloading those outputs. |
| Dependency assurance | The result explicitly remains `ACCEPTED_WITH_IA_TRUST`; the current CLI cannot classify the check-critical transitive IA dependencies. | Fully CA critical inputs, or a reliable transitive CA/IA classifier with strict policy, could permit `ACCEPTED_CA` and fail closed on disallowed IA edges. |
| Signer policy | Exactly one accepted local signer is supported. | Threshold signatures, workload identity, key rotation metadata, and backend-specific authorization could reduce reliance on one producer key. |
| Audit evidence | JSON explains the decision; `.det` can package controlled research evidence but is not yet an xo-nixpkg release contract. | A release could publish deterministic signed recognition evidence alongside the existing assertion, closure graph, SBOMs, and checksums. |

The practical current benefit is therefore integration and measurement: it
defines a small deterministic gate around xo-nixpkg's existing supply evidence,
proves invalidation behavior, and gives protected producers structured
hit/miss/conflict results. It should not yet be used to remove xo-nixpkg's
normal builds from clean-runner CI.

The larger future benefit is downloadless cross-run recognition. Once a public
Nix interface can cryptographically validate remote build traces without
substituting the output, the same check could let xo-nixpkg avoid rebuilding or
downloading a previously accepted multi-package closure while still missing on
source pins, Yarn hashes, Nixpkgs revisions, check logic, or policy changes.
