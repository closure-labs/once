# Once v0.1 implementation plan

## Purpose

Once is a proof of concept for recognizing a previously accepted realization of
the same fully resolved Nix derivation. Nix remains responsible for derivation
identity, resolution, native build traces, and signature verification.

## Phases

1. Record the Nix 2.35 CLI and build-trace behavior in a disposable store.
2. Bootstrap the Rust CLI, configuration, reporting, and Nix adapter.
3. Add the deterministic content-addressed demo and `lib.mkOnceCheck`.
4. Implement fail-closed policy decisions and `check`/`run` orchestration.
5. Demonstrate signed trace reuse, deletion, and invalidation in CI.
6. Complete security documentation and the acceptance matrix.

Every phase has a test gate. No remote trust result is accepted merely because
a signer label appears in JSON: Nix must first accept the trace under its own
trusted-key configuration.

## Public contract

The stable decisions are `ACCEPTED_CA`, `ACCEPTED_WITH_IA_TRUST`, `MISS`,
`UNTRUSTED`, `CONFLICT`, `UNSUPPORTED`, and `ERROR`. Only accepted decisions
permit work to be skipped.

The CLI provides `doctor`, `check`, `resolve`, `trace`, `trust`, `explain`, and
`run`. All commands except `run` are read-only with respect to build outputs.

