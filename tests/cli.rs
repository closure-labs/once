#![cfg(unix)]

use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::Path,
    process::{Command, Output},
};

use tempfile::TempDir;

fn fixture() -> (TempDir, std::path::PathBuf, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory");
    let nix = directory.path().join("nix");
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let script = format!(
        "#!{shell}\n{}",
        r##"set -eu
args="$*"
case "$args" in
  "--version")
    echo 'nix (Nix) 2.35.2'
    ;;
  "config show --json")
    echo '{"experimental-features":{"value":["nix-command","flakes","ca-derivations"]}}'
    ;;
  *"build-trace info --help"*)
    exit 0
    ;;
  *"flake metadata github:closure-labs/once-policy/"*)
    revision="${FAKE_NIX_POLICY_REVISION:-0123456789abcdef0123456789abcdef01234567}"
    echo "{\"revision\":\"$revision\"}"
    ;;
  *"build github:closure-labs/once-policy/"*"#policy --no-link --print-out-paths"*)
    echo '/nix/store/once-policy.toml'
    ;;
  *"store cat /nix/store/once-policy.toml"*)
    cat <<'POLICY'
schema = 1
[once]
policy_version = "protected-v1"
[nix]
minimum_version = "2.35.0"
required_experimental_features = ["nix-command", "flakes", "ca-derivations"]
[trust]
required_signatures = 1
accepted_key_names = ["ci.closurelabs.dev-1"]
ia_mode = "warn"
POLICY
    ;;
  *"derivation show"*)
    echo '{"/nix/store/unresolved-check.drv":{}}'
    ;;
  *"build-trace info"*)
    if [ "${FAKE_NIX_MISS:-}" = 1 ]; then
      echo "error: cannot operate on output 'out' of the unbuilt derivation" >&2
      exit 1
    fi
    echo '[{"key":{"drvPath":"resolved-check.drv","outputName":"out"},"value":{"outPath":"result-check","signatures":[{"keyName":"ci.closurelabs.dev-1","sig":"AA=="}]}},{"opaquePath":"/nix/store/result-check"}]'
    ;;
  *"build"*)
    echo '[]'
    ;;
  *)
    echo "unexpected fake Nix arguments: $args" >&2
    exit 2
    ;;
esac
"##,
    );
    fs::write(&nix, script).expect("fake Nix executable");
    let mut permissions = fs::metadata(&nix).expect("metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&nix, permissions).expect("executable permissions");

    let config = directory.path().join("once.toml");
    fs::write(
        &config,
        r#"schema = 1
[once]
policy_version = "poc-v1"
[nix]
minimum_version = "2.35.0"
required_experimental_features = ["nix-command", "flakes", "ca-derivations"]
[trust]
required_signatures = 1
accepted_key_names = ["ci.closurelabs.dev-1"]
ia_mode = "warn"
"#,
    )
    .expect("configuration");
    (directory, nix, config)
}

fn once(nix: &Path, config: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_once"))
        .env("ONCE_NIX_BIN", nix)
        .args(["--config", config.to_str().expect("UTF-8 path")])
        .args(arguments)
        .output()
        .expect("run Once")
}

fn once_with_external_policy(nix: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_once"))
        .env("ONCE_NIX_BIN", nix)
        .args(arguments)
        .output()
        .expect("run Once")
}

#[test]
fn check_emits_accepted_json() {
    let (_directory, nix, config) = fixture();
    let output = once(&nix, &config, &["--json", "check", ".#demo"]);
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON result envelope");
    assert_eq!(json["decision"], "ACCEPTED_WITH_IA_TRUST");
    assert_eq!(
        json["derivation"]["resolved"],
        "/nix/store/resolved-check.drv"
    );
    assert_eq!(json["trace"]["signers"][0], "ci.closurelabs.dev-1");
}

#[test]
fn cold_check_uses_miss_exit_code() {
    let (_directory, nix, config) = fixture();
    let output = Command::new(env!("CARGO_BIN_EXE_once"))
        .env("ONCE_NIX_BIN", nix)
        .env("FAKE_NIX_MISS", "1")
        .args(["--config", config.to_str().expect("UTF-8 path")])
        .args(["--json", "check", ".#demo"])
        .output()
        .expect("run Once");
    assert_eq!(output.status.code(), Some(10));
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON result envelope");
    assert_eq!(json["decision"], "MISS");
    assert_eq!(json["action"], "BUILD");
}

#[test]
fn doctor_reports_suitable_fake_nix() {
    let (_directory, nix, config) = fixture();
    let output = once(&nix, &config, &["--json", "doctor"]);
    assert!(output.status.success());
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("doctor JSON");
    assert_eq!(json["suitable"], true);
}

#[test]
fn check_loads_policy_from_verified_immutable_flake() {
    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
    let (_directory, nix, _config) = fixture();
    let reference = format!("github:closure-labs/once-policy/{REVISION}");
    let output = once_with_external_policy(
        &nix,
        &[
            "--policy-flake",
            &reference,
            "--policy-revision",
            REVISION,
            "--json",
            "check",
            ".#demo",
        ],
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("JSON result envelope");
    assert_eq!(json["decision"], "ACCEPTED_WITH_IA_TRUST");
}

#[test]
fn external_policy_revision_mismatch_fails_closed() {
    const EXPECTED: &str = "0123456789abcdef0123456789abcdef01234567";
    const ACTUAL: &str = "89abcdef0123456789abcdef0123456789abcdef";
    let (_directory, nix, _config) = fixture();
    let reference = format!("github:closure-labs/once-policy/{EXPECTED}");
    let output = Command::new(env!("CARGO_BIN_EXE_once"))
        .env("ONCE_NIX_BIN", nix)
        .env("FAKE_NIX_POLICY_REVISION", ACTUAL)
        .args([
            "--policy-flake",
            &reference,
            "--policy-revision",
            EXPECTED,
            "check",
            ".#demo",
        ])
        .output()
        .expect("run Once");
    assert_eq!(output.status.code(), Some(30));
    assert!(String::from_utf8_lossy(&output.stderr).contains("expected"));
}

#[test]
fn mutable_policy_reference_is_rejected() {
    const REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
    let (_directory, nix, _config) = fixture();
    let output = once_with_external_policy(
        &nix,
        &[
            "--policy-flake",
            "github:closure-labs/once-policy/main",
            "--policy-revision",
            REVISION,
            "check",
            ".#demo",
        ],
    );
    assert_eq!(output.status.code(), Some(30));
    assert!(String::from_utf8_lossy(&output.stderr).contains("same full commit"));
}
