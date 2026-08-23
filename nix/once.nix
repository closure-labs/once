{
  pkgs,
  name,
  target,
  policyVersion,
  check ? "",
  metadata ? { },
}:
let
  payload = builtins.toJSON {
    schema = "dev.closurelabs.once/check/v1";
    status = "ok";
    policy = policyVersion;
    inherit metadata;
  };
in
pkgs.runCommand "${name}-once-check"
  {
    __contentAddressed = true;
    outputHashAlgo = "sha256";
    outputHashMode = "recursive";
    inherit policyVersion target;
  }
  ''
    test -e "$target"
    ${check}
    mkdir -p "$out"
    printf '%s\n' '${payload}' > "$out/result.json"
  ''
