#!/usr/bin/env bash

set -euo pipefail

repository_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

mkdir -p "$fixture/bin" "$fixture/store/bin"

cat >"$fixture/bin/nix" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
case "$*" in
  *"build path:"*"#once --no-link --print-out-paths")
    printf '%s\n' "$FAKE_ONCE_STORE"
    ;;
  *)
    echo "unexpected fake Nix arguments: $*" >&2
    exit 2
    ;;
esac
EOF

cat >"$fixture/store/bin/once" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
command_name=
for argument in "$@"; do
  case "$argument" in
    doctor | check | run) command_name=$argument ;;
  esac
done
if [[ "$command_name" == doctor ]]; then
  printf '{\n  "schema": "dev.closurelabs.once/doctor/v1",\n  "suitable": true\n}\n'
else
  printf '{\n  "schema": "dev.closurelabs.once/result/v1",\n  "decision": "MISS"\n}\n'
fi
exit "${FAKE_ONCE_EXIT:-0}"
EOF

chmod +x "$fixture/bin/nix" "$fixture/store/bin/once"

run_action() {
  local command_name=$1
  local installable=$2
  local exit_code=$3
  : >"$fixture/output"
  : >"$fixture/summary"
  PATH="$fixture/bin:$PATH" \
    FAKE_ONCE_STORE="$fixture/store" \
    FAKE_ONCE_EXIT="$exit_code" \
    GITHUB_ACTION_PATH="$repository_root" \
    GITHUB_OUTPUT="$fixture/output" \
    GITHUB_STEP_SUMMARY="$fixture/summary" \
    ONCE_ACTION_COMMAND="$command_name" \
    ONCE_ACTION_INSTALLABLE="$installable" \
    ONCE_ACTION_POLICY_FLAKE=github:closure-labs/once-policy/0123456789abcdef0123456789abcdef01234567 \
    ONCE_ACTION_POLICY_REVISION=0123456789abcdef0123456789abcdef01234567 \
    bash "$repository_root/scripts/github-action.sh"
}

run_action doctor "" 0
grep -q '^schema=dev.closurelabs.once/doctor/v1$' "$fixture/output"
grep -q '^exit-code=0$' "$fixture/output"
grep -q '"suitable": true' "$fixture/output"
grep -q 'Once GitHub Action' "$fixture/summary"

run_action check .#integration 10
grep -q '^schema=dev.closurelabs.once/result/v1$' "$fixture/output"
grep -q '^exit-code=10$' "$fixture/output"
grep -q '"decision": "MISS"' "$fixture/output"

if run_action trace .#integration 0 2>/dev/null; then
  echo "invalid action command was accepted" >&2
  exit 1
fi

if run_action check "" 0 2>/dev/null; then
  echo "missing installable was accepted" >&2
  exit 1
fi

if ONCE_ACTION_FAIL_ON_NONZERO=maybe run_action doctor "" 0 2>/dev/null; then
  echo "invalid fail-on-nonzero value was accepted" >&2
  exit 1
fi

echo "GitHub Action wrapper tests passed"
