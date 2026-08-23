{
  description = "Once — recognize accepted Nix build-trace realizations";

  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0";

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      mkOnceCheck = import ./nix/once.nix;
      demo = import ./nix/demo.nix { inherit pkgs mkOnceCheck; };
      once = pkgs.rustPlatform.buildRustPackage {
        pname = "once";
        version = "0.1.1";
        src = pkgs.lib.cleanSource ./.;
        cargoLock.lockFile = ./Cargo.lock;
        meta = {
          description = "Recognize accepted Nix build-trace realizations";
          homepage = "https://github.com/closure-labs/once";
          license = pkgs.lib.licenses.asl20;
          mainProgram = "once";
          platforms = [ system ];
        };
      };
    in
    {
      lib = { inherit mkOnceCheck; };

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
        inherit (demo) demo-expensive demo-once;
      };

      devShells.${system}.default = pkgs.mkShell {
        packages = with pkgs; [
          actionlint
          cargo
          clippy
          jq
          nixfmt
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
