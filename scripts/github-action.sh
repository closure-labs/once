#!/usr/bin/env bash

set -euo pipefail

command_name=${ONCE_ACTION_COMMAND:-}
installable=${ONCE_ACTION_INSTALLABLE:-}
policy_flake=${ONCE_ACTION_POLICY_FLAKE:-}
policy_revision=${ONCE_ACTION_POLICY_REVISION:-}
fail_on_nonzero=${ONCE_ACTION_FAIL_ON_NONZERO:-true}
install_nix=${ONCE_ACTION_INSTALL_NIX:-true}

if [[ "$fail_on_nonzero" != true && "$fail_on_nonzero" != false ]]; then
  echo "fail-on-nonzero must be true or false" >&2
  exit 2
fi
if [[ "$install_nix" != true && "$install_nix" != false ]]; then
  echo "install-nix must be true or false" >&2
  exit 2
fi

case "$command_name" in
  doctor)
    if [[ -n "$installable" ]]; then
      echo "the doctor command does not accept an installable" >&2
      exit 2
    fi
    schema=dev.closurelabs.once/doctor/v1
    ;;
  check | run)
    if [[ -z "$installable" ]]; then
      echo "the $command_name command requires an installable" >&2
      exit 2
    fi
    schema=dev.closurelabs.once/result/v1
    ;;
  *)
    echo "command must be one of: doctor, check, run" >&2
    exit 2
    ;;
esac

if [[ -z "$policy_flake" || -z "$policy_revision" ]]; then
  echo "policy-flake and policy-revision are required" >&2
  exit 2
fi

build_output=$(nix \
  --extra-experimental-features 'nix-command flakes ca-derivations' \
  build "path:$GITHUB_ACTION_PATH#once" \
  --no-link \
  --print-out-paths)

store_paths=()
while IFS= read -r path; do
  [[ -n "$path" ]] && store_paths+=("$path")
done <<<"$build_output"
if [[ ${#store_paths[@]} -ne 1 || ! -x "${store_paths[0]}/bin/once" ]]; then
  echo "Once flake build did not produce exactly one executable package" >&2
  exit 40
fi

once_arguments=(
  --policy-flake "$policy_flake"
  --policy-revision "$policy_revision"
  --json
  "$command_name"
)
if [[ "$command_name" != doctor ]]; then
  once_arguments+=("$installable")
fi

result_file=$(mktemp)
trap 'rm -f "$result_file"' EXIT
set +e
"${store_paths[0]}/bin/once" "${once_arguments[@]}" >"$result_file"
exit_code=$?
set -e
result_json=$(<"$result_file")

write_output() {
  local name=$1
  local value=$2
  local delimiter
  delimiter="once_$(tr -d '-' </proc/sys/kernel/random/uuid)"
  {
    printf '%s<<%s\n' "$name" "$delimiter"
    printf '%s\n' "$value"
    printf '%s\n' "$delimiter"
  } >>"$GITHUB_OUTPUT"
}

write_output result-json "$result_json"
printf 'schema=%s\n' "$schema" >>"$GITHUB_OUTPUT"
printf 'exit-code=%s\n' "$exit_code" >>"$GITHUB_OUTPUT"

if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo "## Once GitHub Action"
    echo
    printf -- '- Command: %s\n' "$command_name"
    printf -- '- Result schema: %s\n' "$schema"
    printf -- '- Exit code: %s\n' "$exit_code"
  } >>"$GITHUB_STEP_SUMMARY"
fi
