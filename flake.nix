{
  description = "Once — recognize accepted Nix build-trace realizations";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      mkOnceCheck = import ./nix/once.nix;
      demo = import ./nix/demo.nix { inherit pkgs mkOnceCheck; };
      once = pkgs.callPackage ./nix/package.nix { };
    in
    {
      lib = { inherit mkOnceCheck; };

      overlays.default = final: _previous: {
        once = final.callPackage ./nix/package.nix { };
      };

      packages.${system} = {
        inherit once;
        default = once;
      };

      apps.${system}.default = {
        type = "app";
        program = "${once}/bin/once";
      };

      checks.${system} = {
        rust-tests = once;
        det-artifact-tests =
          pkgs.runCommand "det-artifact-tests"
            {
              nativeBuildInputs = with pkgs; [
                bash
                coreutils
                jq
                openssl
              ];
            }
            ''
              bash ${self}/tests/det-artifact.sh
              touch $out
            '';
        inherit (demo) demo-expensive demo-once;
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          actionlint
          cargo
          clippy
          jq
          nixfmt
          openssl
          python3
          rustc
          rustfmt
          shellcheck
        ];
        shellHook = ''
          export RUST_SRC_PATH="${pkgs.rustPlatform.rustLibSrc}"
        '';
      };

      formatter.${system} = pkgs.nixfmt;
    };
}
