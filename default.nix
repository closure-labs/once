{
  system ? builtins.currentSystem,
  nixpkgs ? builtins.fetchTarball {
    url = "https://github.com/NixOS/nixpkgs/archive/a831408e6378bc02ebf8cc09b52c96ca86f6bab4.tar.gz";
    sha256 = "sha256-NcYt9QJfpJiF1lAyN8BDPB4EeScbPU+EwQqPiBElrpU=";
  },
  pkgs ? import nixpkgs { inherit system; },
}:

pkgs.callPackage ./nix/package.nix { }
