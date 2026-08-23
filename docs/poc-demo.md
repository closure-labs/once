# Proof-of-concept demo

Run `./scripts/demo.sh` on x86_64 Linux with Nix 2.35+. The script creates
temporary producer and cache stores, generates an ephemeral signing key, and
never modifies the working checkout.

The assertions cover:

1. a cold `MISS`;
2. execution of the expensive derivation and tiny check;
3. a signed accepted trace;
4. publication to a native file binary cache;
5. deletion of the expensive producer output;
6. trace-only recognition without restoring that output;
7. invalidation after relevant source and policy changes;
8. stability after an irrelevant documentation change.

Temporary keys and stores are removed by the script's exit trap.

The script first deletes the resolved check derivation because that store object
references the expensive output. The authoritative base trace has no foreign-key
reference to either object and remains available afterward.

## Nix version comparison

After building the debug binary, run
`./scripts/build-trace-version-matrix.sh` to compare the exact pinned Nix
2.35.2 baseline with the pinned 2.36 prerelease. This separate, heavier proof
uses loopback HTTP and unrelated signing keys to exercise remote build-trace
signature enforcement without modifying host trust configuration.

See the [version-matrix proof](planning/nix-2.35-2.36-build-trace-proof.md) for
the exact pins, security controls, observed results, and limitations.
The matrix can package its successful evidence as a signed
[`.det` artifact](deterministic-artifacts.md).
