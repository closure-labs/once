#!/usr/bin/env bash
# Real native signatures and fresh-store resolution; requires Nix >= 2.35.3.
set -euo pipefail
umask 077

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
once_bin=${ONCE_BIN:-"$repo_root/target/debug/once"}
nix_bin=${ONCE_NIX_BIN:-nix}
for required in "$once_bin" "$nix_bin" jq python3; do
  command -v "$required" >/dev/null || { echo "Required executable missing: $required" >&2; exit 1; }
done
native_tmp=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/once-native.XXXXXXXX")
cleanup() {
  if [[ ${ONCE_NATIVE_KEEP_TMP:-0} == 1 ]]; then
    echo "Native bundle test state: $native_tmp" >&2
  else
    chmod -R u+w "$native_tmp" 2>/dev/null || true
    rm -rf -- "$native_tmp"
  fi
}
trap cleanup EXIT
export XDG_CACHE_HOME="$native_tmp/cache"
export NIX_CONF_DIR="$native_tmp/nix-conf"
export NIX_USER_CONF_FILES=/dev/null
mkdir -p "$NIX_CONF_DIR"
# Never inherit a workstation daemon/store selector or private signing key.
unset NIX_REMOTE ONCE_NIX_EVAL_STORE
export ONCE_NIX_BIN="$nix_bin"
base_config='extra-experimental-features = nix-command flakes ca-derivations
sandbox = true
substituters =
extra-substituters =
builders =
max-jobs = 1'
export NIX_CONFIG="$base_config"
producer_root="$native_tmp/producer"
consumer_root="$native_tmp/consumer"
producer_store="local?root=$producer_root"
consumer_store="local?root=$consumer_root"

builder=${ONCE_NATIVE_BUILDER:-$(readlink -f "$(command -v bash)")}
if [[ "$builder" != /nix/store/*/bin/bash ]]; then
  # Nix installation/development shells normally provide a store-backed bash.
  # The fallback supports workstations whose interactive shell is /usr/bin/bash.
  shopt -s nullglob
  candidates=(/nix/store/*-bash-*/bin/bash /nix/store/*-bash-interactive-*/bin/bash)
  [[ ${#candidates[@]} -gt 0 ]] || { echo 'Set ONCE_NATIVE_BUILDER to an installed /nix/store/.../bin/bash' >&2; exit 1; }
  builder=${candidates[0]}
fi
builder_store=${builder%/bin/bash}
"$nix_bin" copy --to "$producer_store" "$builder_store"
"$nix_bin" copy --to "$consumer_store" "$builder_store"
"$nix_bin" copy --to "local?root=$native_tmp/incomplete-consumer" "$builder_store"

fixture="$native_tmp/fixture"
mkdir -p "$fixture"
printf '%s\n' 'first build input' > "$fixture/input.txt"
printf '%s\n' 'native-v1' > "$fixture/policy-version.txt"
printf '%s\n' '{"nodes":{"root":{}},"root":"root","version":7}' > "$fixture/flake.lock"
# A dependency forces resolution of a graph, rather than one constant trace.
cat > "$fixture/flake.nix" <<'NIX'
{
  outputs = { self }: let
    builder = builtins.getEnv "ONCE_NATIVE_FIXTURE_BUILDER";
    input = builtins.toFile "native-input" (builtins.readFile ./input.txt);
    policyVersion = builtins.replaceStrings [ "\n" ] [ "" ] (builtins.readFile ./policy-version.txt);
    mk = attrs: derivation ({
      system = "x86_64-linux";
      inherit builder;
      __contentAddressed = true;
      outputHashAlgo = "sha256";
      outputHashMode = "recursive";
    } // attrs);
    target = mk {
      name = "once-native-dependency";
      inherit input;
      args = [ "-c" ''start=$SECONDS; while ((SECONDS - start < 2)); do :; done; printf '%s\n' "$(< "$input")" > "$out"'' ];
    };
    check = mk {
      name = "once-native-check";
      inherit target policyVersion;
      args = [ "-c" ''test -s "$target"; printf '%s\n' "$policyVersion" > "$out"'' ];
    };
  in { checks.x86_64-linux.default = check; };
}
NIX
# Substitute the installed shell as a literal: portable evaluation remains pure.
python3 - "$fixture/flake.nix" "$builder" <<'PY'
import json, pathlib, sys
p = pathlib.Path(sys.argv[1])
shell = sys.argv[2]
store = shell.removesuffix('/bin/bash')
expression = 'builtins.appendContext ' + json.dumps(shell) + ' { ' + json.dumps(store) + ' = { path = true; }; }'
p.write_text(p.read_text().replace('builtins.getEnv "ONCE_NATIVE_FIXTURE_BUILDER"', expression))
PY

good_secret="$native_tmp/good-secret"
wrong_secret="$native_tmp/wrong-secret"
"$nix_bin" key generate-secret --key-name once-native-1 > "$good_secret"
"$nix_bin" key generate-secret --key-name once-native-1 > "$wrong_secret"
good_public=$("$nix_bin" key convert-secret-to-public < "$good_secret")
wrong_public=$("$nix_bin" key convert-secret-to-public < "$wrong_secret")
write_policy() {
  local path=$1 public=$2 version=${3:-native-v1}
  cat > "$path" <<POLICY
schema = 1
[once]
policy_version = "$version"
[nix]
minimum_version = "2.35.3"
required_experimental_features = ["nix-command", "flakes", "ca-derivations"]
[trust]
required_signatures = 1
accepted_key_names = ["once-native-1"]
public_keys = ["$public"]
ia_mode = "warn"
POLICY
}
policy="$native_tmp/policy.toml"
write_policy "$policy" "$good_public"
write_policy "$native_tmp/wrong.toml" "$wrong_public"
target="path:$fixture#checks.x86_64-linux.default"
export ONCE_NIX_STORE="$producer_store"
export NIX_CONFIG="$base_config
secret-key-files = $good_secret
extra-trusted-public-keys = $good_public"
echo "Building signed native fixture with $("$nix_bin" --version)" >&2
cold_start=$(python3 -c 'import time; print(time.monotonic())')
"$nix_bin" --store "$producer_store" build "$target" --no-link --json > "$native_tmp/build.json"
cold_end=$(python3 -c 'import time; print(time.monotonic())')
"$nix_bin" --store "$producer_store" store build-trace info "$target" --json > "$native_tmp/native-trace.json"
# Save genuine native compatibility vectors even if a later assertion fails.
if [[ -n ${ONCE_NATIVE_VECTOR_OUTPUT:-} ]]; then
  jq --arg publicKey "$good_public" --arg nixVersion "$("$nix_bin" --version)" \
    '{schema: "dev.closurelabs.once/native-vectors/v1", nixVersion: $nixVersion, publicKey: $publicKey, entries: [.[] | select(.key != null)]}' \
    "$native_tmp/native-trace.json" > "$ONCE_NATIVE_VECTOR_OUTPUT"
fi
# Record only CLI-originated Nix operations; fixture setup/build uses real Nix.
python3 - "$native_tmp/nix-observed" "$(command -v "$nix_bin")" "$native_tmp/nix-operations.jsonl" <<'PYWRAPPER'
import json, pathlib, sys
wrapper, real_nix, log = sys.argv[1:]
program = r"""#!/usr/bin/env python3
import json, os, pathlib, sys
args = sys.argv[1:]
cache_paths = []
for arg in args:
    if arg.startswith('file://'):
        cache = pathlib.Path(arg.removeprefix('file://').split('?', 1)[0])
        for info in cache.glob('*.narinfo'):
            for line in info.read_text().splitlines():
                if line.startswith('StorePath: '):
                    cache_paths.append(line.removeprefix('StorePath: '))
with open(LOG, 'a') as stream:
    stream.write(json.dumps({'argv': args, 'cachePaths': cache_paths}) + '\n')
os.execv(REAL_NIX, [REAL_NIX] + args)
"""
program = program.replace('LOG', repr(log)).replace('REAL_NIX', repr(real_nix))
pathlib.Path(wrapper).write_text(program)
pathlib.Path(wrapper).chmod(0o755)
PYWRAPPER
export ONCE_NIX_BIN="$native_tmp/nix-observed"
"$once_bin" --config "$policy" --json check "$target" > "$native_tmp/local-baseline.json"
jq -e '.decision == "ACCEPTED_WITH_IA_TRUST" and .action == "SKIP"' "$native_tmp/local-baseline.json" >/dev/null
baseline_key=$("$once_bin" --config "$policy" bundle-key "$target")
[[ "$baseline_key" =~ ^once-v1-[0-9a-f]{64}$ ]]
"$once_bin" --config "$policy" export "$target" --output "$native_tmp/bundle"
jq -e '.entries | length >= 2' "$native_tmp/bundle/traces.json" >/dev/null
# Bundles transport only traces, never outputs or store databases.
test "$(find "$native_tmp/bundle" -type f | wc -l)" -eq 1
test "$(du -sk "$native_tmp/bundle" | cut -f1)" -lt 64

# The consumer has only its evaluation shell closure. It has no producer DB,
# realisations, target output NARs, builders, substituters, or signing credentials.
export ONCE_NIX_STORE="$consumer_store"
export NIX_CONFIG="$base_config
max-jobs = 0
extra-trusted-public-keys = $good_public"
assert_outputs_absent() {
  while IFS= read -r output; do
    test ! -e "$consumer_root/nix/store/${output#/nix/store/}"
  done < <(jq -r '.entries[].value.outPath' "$native_tmp/bundle/traces.json")
}
check() {
  local name=$1 expected=$2 config=$3 bundle=$4
  local status=0
  echo "Checking native bundle scenario: $name" >&2
  "$once_bin" --config "$config" --json check "$target" --trace-bundle "$bundle" > "$native_tmp/$name.json" 2> "$native_tmp/$name.stderr" || status=$?
  if [[ "$expected" == hit ]]; then
    test "$status" -eq 0
    jq -e '.decision == "ACCEPTED_WITH_IA_TRUST" and .action == "SKIP"' "$native_tmp/$name.json" >/dev/null
  elif [[ "$expected" == miss ]]; then
    test "$status" -eq 10
    jq -e '.decision == "MISS"' "$native_tmp/$name.json" >/dev/null
  else
    test "$status" -eq 11
    jq -e '.decision == "UNTRUSTED" and .action == "FAIL"' "$native_tmp/$name.json" >/dev/null
  fi
  assert_outputs_absent
}
assert_outputs_absent
warm_start=$(python3 -c 'import time; print(time.monotonic())')
check valid hit "$policy" "$native_tmp/bundle"
warm_end=$(python3 -c 'import time; print(time.monotonic())')
test "$("$once_bin" --config "$policy" bundle-key "$target")" = "$baseline_key"
mkdir "$native_tmp/reversed"
jq '.entries |= reverse' "$native_tmp/bundle/traces.json" > "$native_tmp/reversed/traces.json"
check reversed-records hit "$policy" "$native_tmp/reversed"
check missing miss "$policy" "$native_tmp/absent-bundle"
check wrong-key reject "$native_tmp/wrong.toml" "$native_tmp/bundle"
python3 - "$policy" "$native_tmp/revoked.toml" <<'PYREVOKE'
import pathlib, sys
pathlib.Path(sys.argv[2]).write_text(pathlib.Path(sys.argv[1]).read_text().replace('accepted_key_names = ["once-native-1"]', 'accepted_key_names = ["revoked-1"]'))
PYREVOKE
check revoked-key reject "$native_tmp/revoked.toml" "$native_tmp/bundle"
mkdir "$native_tmp/incomplete"
jq '.entries |= map(select(.key.drvPath | contains("once-native-dependency") | not))' \
  "$native_tmp/bundle/traces.json" > "$native_tmp/incomplete/traces.json"
# Test incomplete evidence both without and with a previously realised local DB.
export ONCE_NIX_STORE="local?root=$native_tmp/incomplete-consumer"
check incomplete-fresh miss "$policy" "$native_tmp/incomplete"
export ONCE_NIX_STORE="$producer_store"
check incomplete-producer miss "$policy" "$native_tmp/incomplete"
export ONCE_NIX_STORE="$consumer_store"
for mutation in signature derivation output-name output-path; do
  mkdir "$native_tmp/$mutation"
  python3 - "$native_tmp/bundle/traces.json" "$native_tmp/$mutation/traces.json" "$mutation" <<'PY'
import base64, json, pathlib, sys
source, destination, mutation = sys.argv[1:]
bundle = json.loads(pathlib.Path(source).read_text())
entry = bundle['entries'][0]
if mutation == 'signature':
    # An accepted signer label paired with a forged signature must fail.
    entry['value']['signatures'][0]['sig'] = base64.b64encode(bytes(64)).decode()
elif mutation == 'derivation':
    entry['key']['drvPath'] = entry['key']['drvPath'].replace('-', '-tampered-', 1)
elif mutation == 'output-name':
    entry['key']['outputName'] = 'other'
else:
    entry['value']['outPath'] += '-tampered'
pathlib.Path(destination).write_text(json.dumps(bundle))
PY
  check "$mutation" reject "$policy" "$native_tmp/$mutation"
done

printf '%s\n' 'irrelevant documentation' > "$fixture/README.md"
check documentation hit "$policy" "$native_tmp/bundle"
test "$("$once_bin" --config "$policy" bundle-key "$target")" = "$baseline_key"
printf '%s\n' 'changed build input' > "$fixture/input.txt"
check relevant-input miss "$policy" "$native_tmp/bundle"
test "$("$once_bin" --config "$policy" bundle-key "$target")" != "$baseline_key"
printf '%s\n' 'first build input' > "$fixture/input.txt"
printf '%s\n' 'native-v2' > "$fixture/policy-version.txt"
write_policy "$native_tmp/policy-v2.toml" "$good_public" native-v2
check policy-version miss "$native_tmp/policy-v2.toml" "$native_tmp/bundle"
test "$("$once_bin" --config "$native_tmp/policy-v2.toml" bundle-key "$target")" != "$baseline_key"

# Assert the CLI never builds/copies and only stages current .drv metadata NARs.
python3 - "$native_tmp/nix-operations.jsonl" <<'PYOPERATIONS'
import json, pathlib, sys
operations = [json.loads(line) for line in pathlib.Path(sys.argv[1]).read_text().splitlines()]
assert operations, 'no CLI Nix invocations observed'
assert any('dump-path' in op['argv'] for op in operations), 'missing derivation staging proof'
assert any(op['cachePaths'] for op in operations), 'missing observed cache narinfo'
for operation in operations:
    args = operation['argv']
    assert not {'build', 'copy', 'realise', 'realize', 'add', 'add-path', 'add-file'}.intersection(args), args
    for path in operation['cachePaths']:
        assert path.endswith('.drv'), f'target NAR published to resolution cache: {path}'
    for arg in args:
        if arg.startswith('/nix/store/'):
            assert arg.split('^', 1)[0].endswith('.drv'), f'target output requested: {args}'
    if 'dump-path' in args:
        operands = args[args.index('dump-path') + 1:]
        assert len(operands) == 1 and operands[0].endswith('.drv'), args
    if 'path-info' in args:
        paths = [arg for arg in args[args.index('path-info') + 1:] if arg.startswith('/nix/store/')]
        assert paths and all(path.endswith('.drv') for path in paths), args
print(f'Observed {len(operations)} CLI Nix invocations: no builds/copies or target output NAR requests')
PYOPERATIONS

python3 - "$cold_start" "$cold_end" "$warm_start" "$warm_end" <<'PYBENCH'
import sys
cold_start, cold_end, warm_start, warm_end = map(float, sys.argv[1:])
print(f"Native synthetic benchmark: cold build {cold_end-cold_start:.3f}s; fresh-store verified reuse {warm_end-warm_start:.3f}s")
PYBENCH
echo 'Native bundle tests passed: fresh-store reuse, signatures, identity tampering, and invalidation.'
