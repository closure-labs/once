#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
once_bin=${ONCE_BIN:-"$repo_root/target/debug/once"}

if [[ ! -x "$once_bin" ]]; then
  nix develop "path:$repo_root" --extra-experimental-features 'nix-command flakes ca-derivations' \
    -c cargo build --manifest-path "$repo_root/Cargo.toml"
fi

demo_tmp_parent=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
demo_tmp=$(mktemp -d -p "$demo_tmp_parent" once-demo.XXXXXXXX)
cleanup() {
  chmod -R u+w "$demo_tmp" 2>/dev/null || true
  rm -rf -- "$demo_tmp"
}
trap cleanup EXIT

producer_root="$demo_tmp/producer"
producer_store="local?root=$producer_root"
cache_dir="$demo_tmp/cache"
cache_store="file://$cache_dir"
secret_key="$demo_tmp/secret-key"
public_key_file="$demo_tmp/public-key"

export NIX_CONFIG="extra-experimental-features = nix-command flakes ca-derivations
sandbox = true"

nix key generate-secret --key-name ci.closurelabs.dev-1 > "$secret_key"
nix key convert-secret-to-public < "$secret_key" > "$public_key_file"
public_key=$(tr -d '\n' < "$public_key_file")

export NIX_CONFIG="$NIX_CONFIG
secret-key-files = $secret_key
extra-trusted-public-keys = $public_key"
export ONCE_NIX_STORE="$producer_store"

target="path:$repo_root#checks.x86_64-linux.demo-once"
expensive="path:$repo_root#checks.x86_64-linux.demo-expensive"

echo "Phase A: cold miss and signed build"
set +e
"$once_bin" --config "$repo_root/.once.toml" --json check "$target" > "$demo_tmp/cold.json"
cold_status=$?
set -e
test "$cold_status" -eq 10
jq -e '.decision == "MISS"' "$demo_tmp/cold.json" > /dev/null

"$once_bin" --config "$repo_root/.once.toml" --json run "$target" > "$demo_tmp/built.json"
jq -e '.decision == "ACCEPTED_WITH_IA_TRUST" and .action == "BUILT"' \
  "$demo_tmp/built.json" > /dev/null

check_output=$(nix --store "$producer_store" build "$target" --no-link --json | jq -r '.[0].outputs.out')
payload_size=$(wc -c < "$producer_root$check_output/result.json")
test "$payload_size" -lt 1024

echo "Phase B: publish native trace, delete the expensive output, and recognize locally"
check_drv=$(nix --store "$producer_store" derivation show "$target" | jq -r '.derivations | keys[0]')
check_drv="/nix/store/$check_drv"
nix --store "$producer_store" copy --to "$cache_store?secret-key=$secret_key" "$check_drv^out"
test -n "$(find "$cache_dir/build-trace-v2" -type f -name '*.doi' -print -quit)"

expensive_output=$(nix --store "$producer_store" build "$expensive" --no-link --json | jq -r '.[0].outputs.out')
resolved_check_drv=$(
  nix --store "$producer_store" store build-trace info "$target" --json \
    | jq -r '[.[] | select(.key != null)][0].key.drvPath'
)
resolved_check_drv="/nix/store/$resolved_check_drv"
nix --store "$producer_store" store delete "$resolved_check_drv"
nix --store "$producer_store" store delete "$expensive_output"
test ! -e "$producer_root$expensive_output"

"$once_bin" --config "$repo_root/.once.toml" --json check "$target" > "$demo_tmp/reused.json"
jq -e '.decision == "ACCEPTED_WITH_IA_TRUST" and .action == "SKIP"' \
  "$demo_tmp/reused.json" > /dev/null
test ! -e "$producer_root$expensive_output"

echo "Phase C-E: irrelevant, relevant, and policy invalidation"
fixture="$demo_tmp/fixture"
mkdir -p "$fixture"
cp "$repo_root/flake.nix" "$repo_root/flake.lock" "$repo_root/Cargo.toml" \
  "$repo_root/Cargo.lock" "$repo_root/.once.toml" "$fixture/"
cp -R "$repo_root/demo" "$repo_root/nix" "$repo_root/src" "$repo_root/docs" "$fixture/"

fixture_target="path:$fixture#checks.x86_64-linux.demo-once"
"$once_bin" --config "$fixture/.once.toml" --json check "$fixture_target" \
  > "$demo_tmp/irrelevant-before.json"
jq -e '.decision == "ACCEPTED_WITH_IA_TRUST"' "$demo_tmp/irrelevant-before.json" > /dev/null

printf '%s\n' 'irrelevant documentation mutation' > "$fixture/docs/irrelevant.md"
"$once_bin" --config "$fixture/.once.toml" --json check "$fixture_target" \
  > "$demo_tmp/irrelevant-after.json"
jq -e '.decision == "ACCEPTED_WITH_IA_TRUST"' "$demo_tmp/irrelevant-after.json" > /dev/null

printf '%s\n' 'build-relevant mutation' > "$fixture/demo/input.txt"
set +e
"$once_bin" --config "$fixture/.once.toml" --json check "$fixture_target" \
  > "$demo_tmp/relevant.json"
relevant_status=$?
set -e
test "$relevant_status" -eq 10
jq -e '.decision == "MISS"' "$demo_tmp/relevant.json" > /dev/null

cp "$repo_root/demo/input.txt" "$fixture/demo/input.txt"
printf '%s\n' 'poc-v2' > "$fixture/demo/policy-version.txt"
set +e
"$once_bin" --config "$fixture/.once.toml" --json check "$fixture_target" \
  > "$demo_tmp/policy.json"
policy_status=$?
set -e
test "$policy_status" -eq 10
jq -e '.decision == "MISS"' "$demo_tmp/policy.json" > /dev/null

echo "Once demo passed: cold miss, signed build, trace reuse after deletion, and invalidation."
