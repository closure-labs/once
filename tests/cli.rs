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
if [ -n "${FAKE_NIX_LOG:-}" ]; then
  printf '%s\n' "$args" >> "$FAKE_NIX_LOG"
fi
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
    mode="${FAKE_NIX_TRACE_MODE:-found}"
    signer="${FAKE_NIX_SIGNER:-ci.closurelabs.dev-1}"
    case "$mode" in
      found)
        echo "[{\"key\":{\"drvPath\":\"resolved-check.drv\",\"outputName\":\"out\"},\"value\":{\"outPath\":\"result-check\",\"signatures\":[{\"keyName\":\"$signer\",\"sig\":\"AA==\"}]}},{\"opaquePath\":\"/nix/store/result-check\"}]"
        ;;
      miss)
        echo "error: cannot operate on output 'out' of the unbuilt derivation" >&2
        exit 1
        ;;
      untrusted)
        echo 'error: build trace signature is invalid' >&2
        exit 1
        ;;
      conflict)
        echo '[{"key":{"drvPath":"resolved-check.drv","outputName":"out"},"value":{"outPath":"result-a","signatures":[{"keyName":"ci.closurelabs.dev-1","sig":"AA=="}]}},{"key":{"drvPath":"resolved-check.drv","outputName":"out"},"value":{"outPath":"result-b","signatures":[{"keyName":"ci.closurelabs.dev-1","sig":"AA=="}]}}]'
        ;;
      malformed)
        echo '{"not":"a trace array"}'
        ;;
      error)
        echo 'error: unexpected upstream failure' >&2
        exit 1
        ;;
      *)
        echo "unknown fake trace mode: $mode" >&2
        exit 2
        ;;
    esac
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
        .env("FAKE_NIX_TRACE_MODE", "miss")
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

fn assert_json_fixture(output: &Output, name: &str) {
    assert!(
        output.status.success(),
        "{name}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("actual JSON output");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/json")
        .join(format!("{name}.json"));
    let expected: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(path).expect("golden JSON fixture"))
            .expect("valid golden JSON");
    assert_eq!(actual, expected, "{name} JSON compatibility changed");
}

#[test]
fn json_outputs_match_compatibility_fixtures() {
    let (_directory, nix, config) = fixture();
    assert_json_fixture(&once(&nix, &config, &["--json", "doctor"]), "doctor");
    for command in ["check", "resolve", "trace", "trust", "explain"] {
        let output = once(&nix, &config, &["--json", command, ".#demo"]);
        assert_json_fixture(&output, command);
    }
}

#[test]
fn read_only_commands_never_request_a_target_build_or_download() {
    let (directory, nix, config) = fixture();
    let log = directory.path().join("nix-invocations.log");
    for mode in ["found", "miss"] {
        for command in ["check", "resolve", "trace", "trust", "explain"] {
            let output = Command::new(env!("CARGO_BIN_EXE_once"))
                .env("ONCE_NIX_BIN", &nix)
                .env("FAKE_NIX_LOG", &log)
                .env("FAKE_NIX_TRACE_MODE", mode)
                .args(["--config", config.to_str().expect("UTF-8 path")])
                .args(["--json", command, ".#must-not-build"])
                .output()
                .expect("run read-only command");
            let expected = if mode == "found" { 0 } else { 10 };
            assert_eq!(output.status.code(), Some(expected), "{command}/{mode}");
        }
    }
    let calls = fs::read_to_string(log).expect("Nix invocation log");
    for forbidden in [" build ", " copy ", " path-info ", " substitute "] {
        assert!(
            !calls.lines().any(|line| line.contains(forbidden)),
            "read-only command invoked `{forbidden}`:\n{calls}"
        );
    }
}

#[test]
fn every_diagnostic_fails_closed_for_ambiguous_trace_states() {
    let (_directory, nix, config) = fixture();
    let cases = [
        ("miss", None, 10, "MISS"),
        ("untrusted", None, 11, "UNTRUSTED"),
        ("conflict", None, 12, "CONFLICT"),
        ("error", None, 40, "ERROR"),
        ("found", Some("unaccepted.example-1"), 11, "UNTRUSTED"),
    ];
    for command in ["check", "resolve", "trace", "trust", "explain"] {
        for (mode, signer, exit_code, decision) in cases {
            let mut invocation = Command::new(env!("CARGO_BIN_EXE_once"));
            invocation
                .env("ONCE_NIX_BIN", &nix)
                .env("FAKE_NIX_TRACE_MODE", mode)
                .args(["--config", config.to_str().expect("UTF-8 path")])
                .args(["--json", command, ".#ambiguous"]);
            if let Some(signer) = signer {
                invocation.env("FAKE_NIX_SIGNER", signer);
            }
            let output = invocation.output().expect("run ambiguous diagnostic");
            assert_eq!(
                output.status.code(),
                Some(exit_code),
                "{command}/{mode}/{signer:?}"
            );
            let json: serde_json::Value =
                serde_json::from_slice(&output.stdout).expect("fail-closed JSON");
            assert_eq!(json["decision"], decision, "{command}/{mode}/{signer:?}");
        }
    }
}

#[test]
fn malformed_trace_and_unverified_remote_fail_closed() {
    let (_directory, nix, config) = fixture();
    for command in ["check", "resolve", "trace", "trust", "explain"] {
        let malformed = Command::new(env!("CARGO_BIN_EXE_once"))
            .env("ONCE_NIX_BIN", &nix)
            .env("FAKE_NIX_TRACE_MODE", "malformed")
            .args(["--config", config.to_str().expect("UTF-8 path")])
            .args(["--json", command, ".#malformed"])
            .output()
            .expect("run malformed diagnostic");
        assert_eq!(malformed.status.code(), Some(40), "{command}/malformed");
        assert!(malformed.stdout.is_empty(), "{command}/malformed");
        assert!(
            String::from_utf8_lossy(&malformed.stderr).contains("could not parse JSON"),
            "{command}/malformed"
        );

        let remote = Command::new(env!("CARGO_BIN_EXE_once"))
            .env("ONCE_NIX_BIN", &nix)
            .env("ONCE_NIX_STORE", "https://cache.example.invalid")
            .args(["--config", config.to_str().expect("UTF-8 path")])
            .args(["--json", command, ".#remote"])
            .output()
            .expect("run remote diagnostic");
        assert_eq!(remote.status.code(), Some(20), "{command}/remote");
        let json: serde_json::Value = serde_json::from_slice(&remote.stdout).expect("remote JSON");
        assert_eq!(json["decision"], "UNSUPPORTED", "{command}/remote");
        if command == "trace" {
            assert_eq!(json["status"], "found");
            assert_eq!(json["evidence"], "unverified-remote");
        }
    }
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
fn diagnostic_commands_have_distinct_json_views() {
    let (_directory, nix, config) = fixture();
    let cases = [
        ("resolve", "dev.closurelabs.once/resolve/v1"),
        ("trace", "dev.closurelabs.once/trace/v1"),
        ("trust", "dev.closurelabs.once/trust/v1"),
        ("explain", "dev.closurelabs.once/result/v1"),
    ];

    for (command, schema) in cases {
        let output = once(&nix, &config, &["--json", command, ".#demo"]);
        assert!(
            output.status.success(),
            "{command}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).expect("diagnostic JSON");
        assert_eq!(json["schema"], schema, "{command}");
        assert_eq!(json["decision"], "ACCEPTED_WITH_IA_TRUST", "{command}");
        match command {
            "resolve" => {
                assert_eq!(json["status"], "resolved");
                assert_eq!(json["outputName"], "out");
                assert!(json.get("trace").is_none());
            }
            "trace" => {
                assert_eq!(json["evidence"], "local-store");
                assert_eq!(json["entries"][0]["signatureCount"], 1);
                assert_eq!(
                    json["entries"][0]["signerKeyNames"][0],
                    "ci.closurelabs.dev-1"
                );
            }
            "trust" => {
                assert_eq!(json["requiredSignatures"], 1);
                assert_eq!(json["acceptedSignatureCount"], 1);
                assert_eq!(json["maySkip"], true);
            }
            "explain" => {
                assert_eq!(json["trace"]["status"], "found");
                assert!(json.get("derivation").is_some());
            }
            _ => unreachable!(),
        }
    }
}

#[test]
fn diagnostic_commands_have_distinct_human_headings() {
    let (_directory, nix, config) = fixture();
    for command in ["resolve", "trace", "trust", "explain"] {
        let output = once(&nix, &config, &[command, ".#demo"]);
        assert!(output.status.success(), "{command}");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with(&format!("Closure Labs — Once {command}")),
            "{command}: {stdout}"
        );
    }
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
