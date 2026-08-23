{
  description = "Closure Labs — protected Once policy";

  inputs.nixpkgs.url = "https://flakehub.com/f/NixOS/nixpkgs/0";

  outputs =
    { nixpkgs, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
      policy = pkgs.writeTextFile {
        name = "once-policy.toml";
        text = builtins.readFile ./policy.toml;
        meta = {
          description = "Closure Labs Once trust policy";
          license = pkgs.lib.licenses.asl20;
          platforms = [ system ];
        };
      };
    in
    {
      lib = {
        schema = 1;
        policyVersion = "protected-v1";
      };

      packages.${system} = {
        inherit policy;
        default = policy;
      };

      checks.${system}.policy = policy;
    };
}
