{
  description = "Once integration example for closure-labs/xo-nixpkg";

  nixConfig = {
    extra-experimental-features = [
      "nix-command"
      "flakes"
      "ca-derivations"
    ];
    extra-substituters = [
      "https://once.cachix.org"
      "https://xen-orchestra-ce.cachix.org"
    ];
    extra-trusted-public-keys = [
      "once.cachix.org-1:UvTATbX24Ign6jp8p/RhF22vwnD/1bHXV3EEg2AMbZY="
      "xen-orchestra-ce.cachix.org-1:WAOajkFLXWTaFiwMbLidlGa5kWB7Icu29eJnYbeMG7E="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    once = {
      url = "github:closure-labs/once/v0.4.2";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    xo-nixpkg = {
      url = "github:closure-labs/xo-nixpkg/v0.9.5";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      once,
      xo-nixpkg,
      ...
    }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
      policy = builtins.fromTOML (builtins.readFile ./once.toml);
      xoPackage = xo-nixpkg.packages.${system}.latest;
      supplyProtector = xo-nixpkg.packages.${system}.supply-protector-latest;

      xoLatestOnce = once.lib.mkOnceCheck {
        inherit pkgs;
        name = "xo-latest-supply";
        target = supplyProtector;
        policyVersion = policy.once.policy_version;
        check = ''
          test -x "${xoPackage}/bin/xo-server"
          (cd "$target" && ${pkgs.coreutils}/bin/sha256sum --check --strict SHA256SUMS)
          ${pkgs.jq}/bin/jq -e --arg root "${xoPackage}" '
            .schemaVersion == 1 and
            .subject.channel == "latest" and
            .subject.storePath == $root and
            .closure.pathCount > 0 and
            .closure.relationshipCount > 0 and
            .distribution.substituter == "https://xen-orchestra-ce.cachix.org"
          ' "$target/assertion.json" >/dev/null
        '';
        metadata = {
          project = "closure-labs/xo-nixpkg";
          channel = "latest";
          validates = [
            "xo-server executable"
            "supply-protector checksums"
            "supply-protector subject"
          ];
        };
      };

      onceXo = pkgs.writeShellApplication {
        name = "once-xo";
        runtimeInputs = [ once.packages.${system}.once ];
        text = ''
          exec once --config ${./once.toml} "$@"
        '';
      };
    in
    {
      checks.${system}.xo-latest-once = xoLatestOnce;

      packages.${system} = {
        inherit onceXo xoLatestOnce;
        default = xoLatestOnce;
      };

      apps.${system}.once = {
        type = "app";
        program = "${onceXo}/bin/once-xo";
      };

      devShells.${system}.default = pkgs.mkShellNoCC {
        packages = [ onceXo ];
      };

      formatter.${system} = pkgs.nixfmt;
    };
}
