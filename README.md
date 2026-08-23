# Once

[![FlakeHub](https://img.shields.io/endpoint?url=https://flakehub.com/f/closure-labs/once/badge)](https://flakehub.com/flake/closure-labs/once)

**Build once. Recognize it thereafter.**

Once is an experimental Closure Labs project for recognizing when an accepted
realization of the same fully resolved Nix derivation has already occurred. It
uses Nix 2.35+ native build traces, content-addressed derivations, and a local
or immutable external trust policy.

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

To install the tagged release into a Nix profile:

```console
$ nix profile install github:closure-labs/once/v0.1.1
$ once --config /path/to/.once.toml doctor
```

To run it without installing, from a checkout containing `.once.toml`:

```console
$ nix run github:closure-labs/once/v0.1.1 -- --config .once.toml doctor
```

Tagged releases are also published as public FlakeHub flakes:

```nix
{
  inputs.once.url = "https://flakehub.com/f/closure-labs/once/0.1.*";
}
```

Starting with v0.1.1, each GitHub release includes an `x86_64-linux` archive
and SHA-256 checksum.

`once check` never intentionally builds the requested installable. `once run`
builds only after a `MISS`, then requires the resulting trace to satisfy policy.

Protected CI can source policy from an immutable external flake instead of
candidate-controlled `.once.toml`:

```console
$ once \
    --policy-flake github:closure-labs/once-policy/$REVISION \
    --policy-revision "$REVISION" \
    check .#checks.x86_64-linux.integration
```

Both arguments require the same full 40-character commit. Branches, tags, and
abbreviated revisions fail closed. See the
[protected-policy guide](docs/protected-policy.md) for the trust boundary and
update flow.

Nix 2.35 does not validate signatures returned by its remote build-trace
inspection command. Once therefore supports skipping from a trusted local
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
[POC demo](docs/poc-demo.md), [protected policy](docs/protected-policy.md),
[governance](docs/governance.md), and the [changelog](CHANGELOG.md) for details.

## Status and roadmap

This is a proof of concept. The protected policy flake is available in v0.2;
future work includes a reusable GitHub Action, durable HTTP/S3/Harmonia backends, stricter
input-addressed dependency policy, multi-signature thresholds, and additional
platforms.

Copyright (C) 2026 Closure Labs and Dale Morgan. Licensed under
[Apache-2.0](LICENSE).
