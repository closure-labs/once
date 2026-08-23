{ pkgs, mkOnceCheck }:
let
  input = builtins.path {
    name = "once-demo-input";
    path = ../demo/input.txt;
  };
  policyVersion = pkgs.lib.strings.removeSuffix "\n" (builtins.readFile ../demo/policy-version.txt);

  demo-expensive =
    pkgs.runCommand "once-demo-expensive"
      {
        __contentAddressed = true;
        outputHashAlgo = "sha256";
        outputHashMode = "recursive";
        inherit input;
      }
      ''
        echo "Once demo: executing the expensive derivation" >&2
        sleep 8
        mkdir -p "$out"
        input_text="$(tr '\n' ' ' < "$input")"
        yes "$input_text" | head -c 4194304 > "$out/payload.bin" || true
        sha256sum "$out/payload.bin" | sed "s|$out/||" > "$out/manifest.sha256"
      '';

  demo-once = mkOnceCheck {
    inherit pkgs;
    name = "demo";
    target = demo-expensive;
    inherit policyVersion;
    check = ''
      test -f "$target/manifest.sha256"
      (cd "$target" && sha256sum --check manifest.sha256)
    '';
    metadata = {
      purpose = "once-poc";
    };
  };
in
{
  inherit demo-expensive demo-once;
}
