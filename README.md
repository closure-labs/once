# Once

**Build once. Recognize it thereafter.**

Once is an experimental Closure Labs project for recognizing when an accepted
realization of the same fully resolved Nix derivation has already occurred. It
uses Nix 2.35+ native build traces, content-addressed derivations, and a local
trust policy.

A passing Once check is a policy decision to accept a signed build-trace claim.
It is not an independently auditable proof that arbitrary computation occurred.

## Requirements

- x86_64 Linux
- upstream Nix 2.35 or later
- `nix-command`, `flakes`, and `ca-derivations`
- stable Rust for source development, provided by `nix develop`

## Quick start

```console
$ nix develop
$ cargo build
$ ./target/debug/once doctor
$ ./scripts/demo.sh
```

`once check` never intentionally builds the requested installable. `once run`
builds only after a `MISS`, then requires the resulting trace to satisfy policy.

Nix 2.35 does not validate signatures returned by its remote build-trace
inspection command. Once v0.1 therefore supports skipping from a trusted local
store and reports remote trace hits as `UNSUPPORTED`; it never trusts a signer
label alone.

```console
$ once check .#checks.x86_64-linux.demo-once
$ once run .#checks.x86_64-linux.demo-once
```

Use `--json` for the versioned `dev.closurelabs.once/result/v1` envelope. Exit
codes distinguish accepted checks, misses, untrusted traces, conflicts,
unsupported environments, bad configuration, and internal failures.

## Architecture

```text
once CLI
  ├── Nix adapter ──> Nix 2.35 evaluation, resolution, and build trace
  ├── policy engine ──> signer, IA-mode, and conflict decisions
  └── reporter ──> terminal, JSON, and GitHub summary
```

See [architecture](docs/architecture.md), [threat model](docs/threat-model.md),
[POC demo](docs/poc-demo.md), and the [changelog](CHANGELOG.md) for details.

## Status and roadmap

This is a proof of concept. Future work includes a protected policy flake,
reusable GitHub Action, durable HTTP/S3/Harmonia backends, stricter
input-addressed dependency policy, multi-signature thresholds, and additional
platforms.

Licensed under Apache-2.0.
