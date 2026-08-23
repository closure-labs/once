# Use Once as a Nix package

Once publishes both a conventional Nix package expression and a flake package
directly from the Closure Labs GitHub repository. The package does not depend
on Once being included in the upstream Nixpkgs collection. Official Nixpkgs
submission and release are planned as a long-term milestone after Once has
matured; until then, this repository is the authoritative package source.

The current package supports Linux. Flake commands require the `nix-command`
and `flakes` experimental features. Once operations additionally require the
`ca-derivations` feature.

## Build the conventional Nix package

The repository's `default.nix` pins its own community Nixpkgs revision and
calls the reusable `nix/package.nix` expression. From a checkout:

```console
nix-build
./result/bin/once --version
```

Consumers that already have a Nixpkgs package set can call the package
expression directly. This example fetches the package source from the v0.4.0
tag rather than requiring Once to exist in Nixpkgs:

```nix
{ pkgs ? import <nixpkgs> { } }:

let
  onceSource = pkgs.fetchFromGitHub {
    owner = "closure-labs";
    repo = "once";
    # v0.4.0 is used here because its immutable source hash is known in
    # advance; update both fields together when selecting another release.
    rev = "v0.4.0";
    hash = "sha256-W/yA/0+USGRfJLwzmR8daaD0wF6LuEOXblnO7Itlv+U=";
  };
in
pkgs.callPackage "${onceSource}/nix/package.nix" { }
```

Use the downstream package set's own pin in production. The top-level
`default.nix` is provided for direct repository builds and keeps its Nixpkgs
revision aligned with `flake.lock`.

## Install, run, or build the flake package

Install Once into the current user's Nix profile:

```console
nix profile install github:closure-labs/once/v0.4.1#once
once --version
```

Run it without installing:

```console
nix run github:closure-labs/once/v0.4.1#once -- \
  --config /path/to/.once.toml doctor
```

Build it into the local `result` symlink:

```console
nix build github:closure-labs/once/v0.4.1#once
./result/bin/once --version
```

The `#once` selector is explicit; it currently resolves to the same derivation
as the flake's default package and app.

## Use the package from another flake

Follow the downstream project's Nixpkgs input so Once is built with the same
package set:

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
    in {
      packages.${system}.default = once.packages.${system}.once;
    };
}
```

## Add `pkgs.once` with the overlay

The repository exports `overlays.default` for projects that want Once in their
existing Nixpkgs package set:

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

## Install on NixOS

Apply the overlay and add `pkgs.once` to `environment.systemPackages`:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    once.url = "github:closure-labs/once/v0.4.1";
    once.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = { nixpkgs, once, ... }: {
    nixosConfigurations.example = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ({ pkgs, ... }: {
          nixpkgs.overlays = [ once.overlays.default ];
          environment.systemPackages = [ pkgs.once ];
        })
      ];
    };
  };
}
```

## Update a pinned input

Change the tag in `inputs.once.url`, then refresh only that input:

```console
nix flake lock --update-input once
nix build .#
```

Review the resulting `flake.lock` change before committing it. Tags provide a
human-readable release pin; `flake.lock` records the exact source revision and
content hash used by the downstream project.
