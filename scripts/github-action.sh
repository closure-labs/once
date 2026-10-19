#!/usr/bin/env bash
set -euo pipefail

command_name=${ONCE_ACTION_COMMAND:-}
installable=${ONCE_ACTION_INSTALLABLE:-}
policy_flake=${ONCE_ACTION_POLICY_FLAKE:-}
policy_revision=${ONCE_ACTION_POLICY_REVISION:-}
fail_on_nonzero=${ONCE_ACTION_FAIL_ON_NONZERO:-true}
install_nix=${ONCE_ACTION_INSTALL_NIX:-true}
trace_bundle=${ONCE_ACTION_TRACE_BUNDLE:-}
producer_workflow=${ONCE_ACTION_PRODUCER_WORKFLOW:-}
producer_branch=${ONCE_ACTION_PRODUCER_BRANCH:-main}
ONCE_ACTION_ROOT=${GITHUB_ACTION_PATH:?}
# shellcheck source=scripts/github-common.sh
source "$ONCE_ACTION_ROOT/scripts/github-common.sh"

for boolean in "$fail_on_nonzero" "$install_nix"; do
  if [[ "$boolean" != true && "$boolean" != false ]]; then
    echo "fail-on-nonzero and install-nix must be true or false" >&2
    exit 2
  fi
done
case "$command_name" in
  doctor)
    [[ -z "$installable" ]] || { echo "doctor does not accept an installable" >&2; exit 2; }
    schema=dev.closurelabs.once/doctor/v1 ;;
  check | run)
    [[ -n "$installable" ]] || { echo "$command_name requires an installable" >&2; exit 2; }
    schema=dev.closurelabs.once/result/v1 ;;
  *) echo "command must be one of: doctor, check, run" >&2; exit 2 ;;
esac
if [[ -z "$policy_flake" || -z "$policy_revision" ]]; then
  echo "policy-flake and policy-revision are required" >&2
  exit 2
fi
if [[ -n "$trace_bundle" && -n "$producer_workflow" ]]; then
  echo "trace-bundle and producer-workflow are mutually exclusive" >&2
  exit 2
fi
if [[ "$command_name" != check && ( -n "$trace_bundle" || -n "$producer_workflow" ) ]]; then
  echo "trace-bundle and producer-workflow require command: check" >&2
  exit 2
fi

once_bootstrap
workspace=$(mktemp -d)
trap 'rm -rf "$workspace"' EXIT
bundle_id=
if [[ -n "$trace_bundle" || -n "$producer_workflow" ]]; then
  once_bundle_key
fi
if [[ -n "$producer_workflow" ]]; then
  set +e
  python3 "$ONCE_ACTION_ROOT/scripts/github-artifacts.py" \
    --workflow "$producer_workflow" --branch "$producer_branch" \
    --name "$bundle_id" --output "$workspace/bundle"
  transport_status=$?
  set -e
  case "$transport_status" in
    0) trace_bundle="$workspace/bundle" ;;
    10) trace_bundle="$workspace/absent" ;;
    *) exit "$transport_status" ;;
  esac
fi
once_arguments=(--policy-flake "$policy_flake" --policy-revision "$policy_revision" --json "$command_name")
[[ "$command_name" == doctor ]] || once_arguments+=("$installable")
[[ -z "$trace_bundle" ]] || once_arguments+=(--trace-bundle "$trace_bundle")
set +e
"$once_bin" "${once_arguments[@]}" >"$workspace/result.json"
exit_code=$?
set -e
result_json=$(<"$workspace/result.json")
cache_hit=false
decision=
if [[ "$command_name" != doctor && -n "$result_json" ]]; then
  decision=$(printf '%s' "$result_json" | python3 -c 'import json,sys; v=json.load(sys.stdin).get("decision", ""); assert isinstance(v,str); print(v)' ) || exit 40
  case "$decision:$exit_code" in
    ACCEPTED_CA:0 | ACCEPTED_WITH_IA_TRUST:0) cache_hit=true ;;
    MISS:10 | UNTRUSTED:11 | UNSUPPORTED:13 | CONFLICT:12 | UNSUPPORTED:20 | ERROR:40) ;;
    *)
      # Never let fail-on-nonzero:false turn inconsistent or unknown results into hits.
      echo "Once returned an unknown or inconsistent decision/exit-code pair: $decision/$exit_code" >&2
      exit_code=40 ;;
  esac
fi
once_output result-json "$result_json"
printf 'schema=%s\nexit-code=%s\ncache-hit=%s\ndecision=%s\nbundle-id=%s\n' \
  "$schema" "$exit_code" "$cache_hit" "$decision" "$bundle_id" >>"$GITHUB_OUTPUT"
if [[ -n "${GITHUB_STEP_SUMMARY:-}" ]]; then
  {
    echo '## Once GitHub Action'
    printf '\n- Command: %s\n- Exit code: %s\n- Accepted reuse: %s\n' "$command_name" "$exit_code" "$cache_hit"
  } >>"$GITHUB_STEP_SUMMARY"
fi
