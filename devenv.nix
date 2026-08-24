{ pkgs, ... }:

{
  cachix.pull = [ "once" ];

  packages = import ./nix/dev-tools.nix { inherit pkgs; };

  env.RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
}
