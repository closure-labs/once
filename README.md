# Once

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

Once is distributed as both a conventional Nix package expression and a flake
package directly from this repository. The current package builds v0.4.1.

From a repository checkout, build the conventional package expression:

```console
$ nix-build
$ ./result/bin/once --version
```

Or install the tagged flake package into a Nix profile:

```console
$ nix profile install github:closure-labs/once/v0.4.1#once
$ once --config /path/to/.once.toml doctor
```

Run the same package without installing it:

```console
$ nix run github:closure-labs/once/v0.4.1#once -- \
    --config /path/to/.once.toml doctor
```

Build the package into `./result`:

```console
$ nix build github:closure-labs/once/v0.4.1#once
$ ./result/bin/once --version
```

Pin a release from this repository as a downstream flake input:

```nix
{
  inputs.once.url = "github:closure-labs/once/v0.4.1";
}
```

Use `once.packages.${system}.once` directly, or add
`once.overlays.default` to a Nixpkgs package set to expose `pkgs.once`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    once.url = "github:closure-labs/once/v0.4.1";
    once.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, once, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ once.overlays.default ];
      };
    in {
      packages.${system}.default = pkgs.once;
    };
}
```

See the [Nix package guide](docs/nix-package.md) for downstream flake and NixOS
examples for both package interfaces. The GitHub repository and its tagged
revisions are the authoritative package source during the proof-of-concept and
early-maturity phases. An official Nixpkgs submission is a long-term release
milestone after Once has matured; it is not the current distribution path.
Starting with v0.1.1, each GitHub release also includes an `x86_64-linux`
archive and SHA-256 checksum.

For source development:

```console
$ git clone https://github.com/closure-labs/once.git
$ cd once
$ nix develop
$ cargo build
$ ./target/debug/once doctor
$ ./scripts/demo.sh
```

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
label alone. See the [v0.2 upstream trust audit](docs/planning/remote-trust-audit.md)
for the audited interfaces and reopening criteria. A reproducible
[2.35.2/2.36 prerelease proof](docs/planning/nix-2.35-2.36-build-trace-proof.md)
demonstrates the newer internal substitution check while confirming that the
read-only inspection boundary is unchanged. Successful matrix evidence can be
packaged and independently verified as a deterministic signed
[`.det` artifact](docs/deterministic-artifacts.md).

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
[diagnostic commands](docs/diagnostic-commands.md), [GitHub Action](docs/github-action.md),
[governance](docs/governance.md), and the [changelog](CHANGELOG.md) for details.

## Status and roadmap

This is a proof of concept. v0.4 provides a repository-hosted Nix package and
Nixpkgs-compatible overlay on top of the substitution evidence, deterministic
signed `.det` artifacts, protected policy flake, and reusable GitHub Action
delivered in earlier releases. Future work includes durable HTTP/S3/Harmonia
backends, stricter input-addressed dependency policy, multi-signature
thresholds, and additional platforms. Official inclusion in Nixpkgs is planned
after these interfaces and their security behavior have matured.

Copyright (C) 2026 Closure Labs and Dale Morgan. Licensed under
[Apache-2.0](LICENSE).
