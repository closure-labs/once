#!/usr/bin/env bash
set -euo pipefail
installable=${ONCE_ACTION_INSTALLABLE:-}
policy_flake=${ONCE_ACTION_POLICY_FLAKE:-}
policy_revision=${ONCE_ACTION_POLICY_REVISION:-}
producer_branch=${ONCE_ACTION_PRODUCER_BRANCH:-main}
# Protection is enforced again here, before evaluation or export.
if [[ "${GITHUB_EVENT_NAME:-}" != push || "${GITHUB_REF:-}" != "refs/heads/$producer_branch" || "${GITHUB_REF_PROTECTED:-}" != true ]]; then
  echo "Bundles may only be published by a push to the configured protected branch" >&2
  exit 40
fi
if [[ -z "$installable" || -z "$policy_flake" || -z "$policy_revision" ]]; then
  echo "installable, policy-flake and policy-revision are required" >&2
  exit 2
fi
ONCE_ACTION_ROOT=$(cd -- "${GITHUB_ACTION_PATH:?}/.." && pwd)
# shellcheck source=scripts/github-common.sh
source "$ONCE_ACTION_ROOT/scripts/github-common.sh"
once_bootstrap
once_bundle_key
bundle_parent=$(mktemp -d "${RUNNER_TEMP:-/tmp}/once-publish.XXXXXXXX")
bundle_directory="$bundle_parent/bundle"
"$once_bin" --policy-flake "$policy_flake" --policy-revision "$policy_revision" \
  export "$installable" --output "$bundle_directory"
printf 'bundle-id=%s\nbundle-directory=%s\n' "$bundle_id" "$bundle_directory" >>"$GITHUB_OUTPUT"
