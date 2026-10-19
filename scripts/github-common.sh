#!/usr/bin/env bash
# Shared by the check and publish composite actions. Source with set -euo pipefail.
once_bootstrap() {
  local version build_output
  version=$(nix --version)
  if [[ "${version##* }" != 2.35.3 ]]; then
    echo "Once portable traces require tested upstream Nix 2.35.3 (Determinate 3.23.3); found: $version" >&2
    return 40
  fi
  build_output=$(nix \
    --extra-experimental-features 'nix-command flakes ca-derivations' \
    --option extra-substituters https://once.cachix.org \
    --option extra-trusted-public-keys 'once.cachix.org-1:UvTATbX24Ign6jp8p/RhF22vwnD/1bHXV3EEg2AMbZY=' \
    --option accept-flake-config false \
    build "path:$ONCE_ACTION_ROOT#once" --no-link --print-out-paths --no-write-lock-file)
  local -a store_paths=()
  local path
  while IFS= read -r path; do
    [[ -n "$path" ]] && store_paths+=("$path")
  done <<<"$build_output"
  if [[ ${#store_paths[@]} -ne 1 || ! -x "${store_paths[0]}/bin/once" ]]; then
    echo "Once flake build did not produce exactly one executable package" >&2
    return 40
  fi
  once_bin="${store_paths[0]}/bin/once"
}

once_output() {
  local name=$1 value=$2 delimiter
  delimiter="once_$(tr -d '-' </proc/sys/kernel/random/uuid)"
  {
    printf '%s<<%s\n' "$name" "$delimiter"
    printf '%s\n' "$value"
    printf '%s\n' "$delimiter"
  } >>"$GITHUB_OUTPUT"
}

once_bundle_key() {
  # Supplied by both composite-action entry points.
  # shellcheck disable=SC2154
  bundle_id=$("$once_bin" --policy-flake "$policy_flake" --policy-revision "$policy_revision" bundle-key "$installable")
  if [[ ! "$bundle_id" =~ ^once-[a-zA-Z0-9._-]+$ || ${#bundle_id} -gt 200 ]]; then
    echo "Once returned an invalid bundle artifact name" >&2
    return 40
  fi
}
