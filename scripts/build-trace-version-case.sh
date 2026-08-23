#!/usr/bin/env bash
set -euo pipefail
umask 077

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
nix_bin=${ONCE_MATRIX_NIX_BIN:-nix}
once_bin=${ONCE_BIN:-"$repo_root/target/debug/once"}

if [[ ! -x "$once_bin" ]]; then
  echo "Once binary is not executable: $once_bin" >&2
  exit 1
fi
for command in jq python3; do
  if ! command -v "$command" > /dev/null; then
    echo "$command is required" >&2
    exit 1
  fi
done

matrix_tmp_parent=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
matrix_tmp=$(mktemp -d -p "$matrix_tmp_parent" once-build-trace-matrix.XXXXXXXX)
cache_server_pid=''
cleanup() {
  if [[ -n "$cache_server_pid" ]]; then
    kill "$cache_server_pid" 2>/dev/null || true
    wait "$cache_server_pid" 2>/dev/null || true
  fi
  if [[ ${ONCE_MATRIX_KEEP_TMP:-0} == 1 ]]; then
    echo "Kept matrix state at $matrix_tmp" >&2
    return
  fi
  chmod -R u+w "$matrix_tmp" 2>/dev/null || true
  rm -rf -- "$matrix_tmp"
}
trap cleanup EXIT

producer_root="$matrix_tmp/producer"
evaluation_root="$matrix_tmp/evaluation"
wrong_consumer_root="$matrix_tmp/consumer-wrong-key"
correct_consumer_root="$matrix_tmp/consumer-correct-key"
producer_store="local?root=$producer_root"
evaluation_store="local?root=$evaluation_root&require-sigs=true"
wrong_consumer_store="local?root=$wrong_consumer_root&require-sigs=true"
correct_consumer_store="local?root=$correct_consumer_root&require-sigs=true"
cache_dir="$matrix_tmp/cache"
cache_write_store="file://$cache_dir"

good_secret_key="$matrix_tmp/good-secret-key"
good_public_key_file="$matrix_tmp/good-public-key"
wrong_secret_key="$matrix_tmp/wrong-secret-key"
wrong_public_key_file="$matrix_tmp/wrong-public-key"

base_nix_config='extra-experimental-features = nix-command flakes ca-derivations
sandbox = true'

env NIX_CONFIG="$base_nix_config" \
  "$nix_bin" key generate-secret --key-name ci.closurelabs.dev-1 > "$good_secret_key"
env NIX_CONFIG="$base_nix_config" \
  "$nix_bin" key convert-secret-to-public < "$good_secret_key" > "$good_public_key_file"
env NIX_CONFIG="$base_nix_config" \
  "$nix_bin" key generate-secret --key-name unrelated.example-1 > "$wrong_secret_key"
env NIX_CONFIG="$base_nix_config" \
  "$nix_bin" key convert-secret-to-public < "$wrong_secret_key" > "$wrong_public_key_file"

good_public_key=$(tr -d '\n' < "$good_public_key_file")
wrong_public_key=$(tr -d '\n' < "$wrong_public_key_file")
producer_nix_config="$base_nix_config
secret-key-files = $good_secret_key
extra-trusted-public-keys = $good_public_key"
correct_key_config="$base_nix_config
extra-trusted-public-keys = $good_public_key"
wrong_key_config="$base_nix_config
extra-trusted-public-keys = $wrong_public_key"

nix_version=$(env NIX_CONFIG="$base_nix_config" "$nix_bin" --version)
target="path:$repo_root#checks.x86_64-linux.demo-once"

echo "Building and publishing with $nix_version" >&2
env NIX_CONFIG="$producer_nix_config" \
  "$nix_bin" --store "$producer_store" build "$target" --no-link > /dev/null

check_drv=$(
  env NIX_CONFIG="$producer_nix_config" \
    "$nix_bin" --store "$producer_store" derivation show "$target" \
    | jq -r '.derivations | keys[0]'
)
check_drv="/nix/store/${check_drv#/nix/store/}"

env NIX_CONFIG="$producer_nix_config" \
  "$nix_bin" --store "$producer_store" store build-trace info "$target" --json \
  > "$matrix_tmp/producer-trace.json"
resolved_drv=$(
  jq -r '[.[] | select(.key != null)][0].key.drvPath' "$matrix_tmp/producer-trace.json"
)
resolved_drv="/nix/store/${resolved_drv#/nix/store/}"
out_path=$(
  jq -r '[.[] | select(.value != null)][0].value.outPath' "$matrix_tmp/producer-trace.json"
)
out_path="/nix/store/${out_path#/nix/store/}"
resolved_installable="$resolved_drv^out"
unresolved_installable="$check_drv^out"

# Give evaluation an opaque derivation closure, but no realised output or build
# trace. Substitution must therefore resolve the CA derivation through the
# remote trace instead of inheriting the producer's local realisation metadata.
env NIX_CONFIG="$producer_nix_config" \
  "$nix_bin" --store "$producer_store" copy \
  --to "$evaluation_store" "$check_drv" > /dev/null
test ! -e "$evaluation_root$out_path"

env NIX_CONFIG="$producer_nix_config" \
  "$nix_bin" --store "$producer_store" copy \
  --to "$cache_write_store?secret-key=$good_secret_key" "$check_drv^out" > /dev/null
env NIX_CONFIG="$producer_nix_config" \
  "$nix_bin" --store "$producer_store" copy \
  --to "$cache_write_store?secret-key=$good_secret_key" "$resolved_drv" > /dev/null
test -n "$(find "$cache_dir/build-trace-v2" -type f -name '*.doi' -print -quit)"

cache_port_file="$matrix_tmp/cache-server.port"
python3 -c '
import functools
import http.server
import os
import pathlib
import sys

cache_dir, port_file = sys.argv[1:]
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=cache_dir)
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
temporary_port_file = f"{port_file}.tmp"
pathlib.Path(temporary_port_file).write_text(str(server.server_port), encoding="ascii")
os.replace(temporary_port_file, port_file)
server.serve_forever()
' "$cache_dir" "$cache_port_file" \
  > "$matrix_tmp/cache-server.log" 2>&1 &
cache_server_pid=$!
cache_port=''
for _ in {1..50}; do
  if [[ -s "$cache_port_file" ]]; then
    cache_port=$(< "$cache_port_file")
    break
  fi
  if ! kill -0 "$cache_server_pid" 2> /dev/null; then
    echo "Loopback cache server exited before publishing its port" >&2
    exit 1
  fi
  sleep 0.1
done
test -n "$cache_port"
cache_url="http://127.0.0.1:$cache_port"
cache_store="$cache_url?trusted=false"
cache_ready=false
for _ in {1..50}; do
  if python3 -c '
import sys
import urllib.request
urllib.request.urlopen(sys.argv[1]).read()
' "$cache_url/nix-cache-info" 2> /dev/null; then
    cache_ready=true
    break
  fi
  sleep 0.1
done
test "$cache_ready" = true

query_remote_trace() {
  local nix_config=$1
  local output=$2
  env NIX_CONFIG="$nix_config" \
    "$nix_bin" --store "$cache_store" store build-trace info "$resolved_installable" --json \
    --eval-store "$producer_store" > "$output"
}

echo "Querying the remote trace with accepted and unrelated keys" >&2
query_remote_trace "$correct_key_config" "$matrix_tmp/remote-correct.json"
query_remote_trace "$wrong_key_config" "$matrix_tmp/remote-wrong.json"
jq -S . "$matrix_tmp/remote-correct.json" > "$matrix_tmp/remote-correct.canonical.json"
jq -S . "$matrix_tmp/remote-wrong.json" > "$matrix_tmp/remote-wrong.canonical.json"
cmp -s "$matrix_tmp/remote-correct.canonical.json" "$matrix_tmp/remote-wrong.canonical.json"
jq -e --arg drv "${resolved_drv#/nix/store/}" --arg out "${out_path#/nix/store/}" '
  any(.[]; .key.drvPath == $drv and .value.outPath == $out)
' "$matrix_tmp/remote-wrong.json" > /dev/null

set +e
env NIX_CONFIG="$wrong_key_config" \
  ONCE_NIX_BIN="$nix_bin" \
  ONCE_NIX_STORE="$cache_store" \
  ONCE_NIX_EVAL_STORE="$producer_store" \
  "$once_bin" --config "$repo_root/.once.toml" --json check "$resolved_installable" \
  > "$matrix_tmp/once-remote.json" 2> "$matrix_tmp/once-remote.stderr"
once_remote_status=$?
set -e
test "$once_remote_status" -eq 20
jq -e '.decision == "UNSUPPORTED" and .trace.status == "found-unverified"' \
  "$matrix_tmp/once-remote.json" > /dev/null

out_hash=${out_path#/nix/store/}
out_hash=${out_hash%%-*}
out_nar_url=$(sed -n 's/^URL: //p' "$cache_dir/$out_hash.narinfo")
test -n "$out_nar_url"
if grep -Fq "GET /$out_nar_url " "$matrix_tmp/cache-server.log"; then
  echo "Read-only trace inspection unexpectedly downloaded $out_path" >&2
  exit 1
fi

wrong_substitution_config="$wrong_key_config
substituters = $cache_store
builders =
max-jobs = 0
fallback = false"
correct_substitution_config="$correct_key_config
substituters = $cache_store
builders =
max-jobs = 0
fallback = false"

echo "Attempting substitute-only consumption with the unrelated key" >&2
set +e
env NIX_CONFIG="$wrong_substitution_config" \
  "$nix_bin" --store "$wrong_consumer_store" build "$unresolved_installable" --no-link \
  --eval-store "$evaluation_store" > "$matrix_tmp/wrong-substitution.stdout" \
  2> "$matrix_tmp/wrong-substitution.stderr"
wrong_substitution_status=$?
set -e

wrong_key_realized_output=false
if [[ -e "$wrong_consumer_root$out_path" ]]; then
  wrong_key_realized_output=true
fi

build_trace_signature_rejected=false
if grep -Fq 'ignoring substitute for build trace' "$matrix_tmp/wrong-substitution.stderr"; then
  build_trace_signature_rejected=true
fi

echo "Attempting substitute-only consumption with the accepted key" >&2
test ! -e "$correct_consumer_root$out_path"
env NIX_CONFIG="$correct_substitution_config" \
  "$nix_bin" --store "$correct_consumer_store" build "$unresolved_installable" --no-link \
  --eval-store "$evaluation_store" > "$matrix_tmp/correct-substitution.stdout" \
  2> "$matrix_tmp/correct-substitution.stderr"
test -e "$correct_consumer_root$out_path"

jq -n \
  --arg schema 'dev.closurelabs.once/build-trace-version-case/v1' \
  --arg nixVersion "$nix_version" \
  --arg unresolvedDerivation "$check_drv" \
  --arg resolvedDerivation "$resolved_drv" \
  --arg outputPath "$out_path" \
  --argjson onceRemoteExitCode "$once_remote_status" \
  --argjson wrongKeySubstitutionExitCode "$wrong_substitution_status" \
  --argjson buildTraceSignatureRejected "$build_trace_signature_rejected" \
  --argjson wrongKeyRealizedOutput "$wrong_key_realized_output" \
  '{
    schema: $schema,
    nixVersion: $nixVersion,
    unresolvedDerivation: $unresolvedDerivation,
    resolvedDerivation: $resolvedDerivation,
    outputPath: $outputPath,
    readOnlyRemoteQuery: {
      correctKeyReturnedTrace: true,
      unrelatedKeyReturnedIdenticalTrace: true,
      referencedOutputNarRequested: false
    },
    onceRemoteDecision: {
      decision: "UNSUPPORTED",
      exitCode: $onceRemoteExitCode
    },
    substitution: {
      consumerRequireSignatures: true,
      substituterTrusted: false,
      unrelatedKeyExitCode: $wrongKeySubstitutionExitCode,
      buildTraceSignatureRejected: $buildTraceSignatureRejected,
      unrelatedKeyRealizedOutput: $wrongKeyRealizedOutput,
      correctKeyRealizedOutput: true
    }
  }'
