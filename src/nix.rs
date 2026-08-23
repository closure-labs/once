use std::{collections::BTreeMap, ffi::OsStr, process::Command};

use semver::Version;
use serde::Deserialize;

use crate::{
    backend::{BackendDescription, EnvironmentBackend, TraceBackend},
    config::Config,
    error::{OnceError, Result},
    trace::TraceRecords,
};

const FEATURES: &str = "nix-command flakes ca-derivations";

#[derive(Debug)]
struct CommandOutput {
    success: bool,
    status: i32,
    stdout: String,
    stderr: String,
}

#[derive(Clone, Debug, Default)]
pub struct Evaluation {
    pub unresolved_drv: Option<String>,
}

#[derive(Clone, Debug)]
pub enum TraceQuery {
    Found {
        records: TraceRecords,
        evidence: TrustEvidence,
    },
    Miss(String),
    Untrusted(String),
    Failed(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrustEvidence {
    LocalStore,
    UnverifiedRemote,
}

#[derive(Clone, Debug)]
pub struct DoctorState {
    pub version: Version,
    pub minimum: Version,
    pub build_trace_command: bool,
    pub ca_derivations_configured: bool,
    pub required_features: Vec<String>,
    pub backend: BackendDescription,
}

impl DoctorState {
    #[must_use]
    pub fn suitable(&self) -> bool {
        self.version >= self.minimum && self.build_trace_command && self.ca_derivations_configured
    }
}

pub struct NixAdapter {
    executable: String,
    backend: EnvironmentBackend,
    store: Option<String>,
    eval_store: Option<String>,
}

impl Default for NixAdapter {
    fn default() -> Self {
        Self {
            executable: std::env::var("ONCE_NIX_BIN").unwrap_or_else(|_| "nix".into()),
            backend: EnvironmentBackend,
            store: std::env::var("ONCE_NIX_STORE").ok(),
            eval_store: std::env::var("ONCE_NIX_EVAL_STORE").ok(),
        }
    }
}

impl NixAdapter {
    pub fn version(&self) -> Result<Version> {
        let output = self.run_raw(["--version"])?;
        if !output.success {
            return Err(self.failed(output));
        }
        parse_nix_version(output.stdout.trim())
    }

    pub fn doctor(&self, config: &Config) -> Result<DoctorState> {
        let version = self.version()?;
        let help = self.run_nix(["store", "build-trace", "info", "--help"])?;
        let configured = self.configured_features()?;
        Ok(DoctorState {
            version,
            minimum: config.nix.minimum_version.clone(),
            build_trace_command: help.success,
            ca_derivations_configured: configured.iter().any(|f| f == "ca-derivations"),
            required_features: config.nix.required_experimental_features.clone(),
            backend: self.backend.describe(),
        })
    }

    pub fn load_policy(&self, reference: &str, expected_revision: &str) -> Result<String> {
        validate_policy_reference(reference, expected_revision)?;

        let metadata = self.run_nix(["flake", "metadata", reference, "--json"])?;
        if !metadata.success {
            return Err(self.failed(metadata));
        }
        let actual_revision = parse_flake_revision(&metadata.stdout)?;
        if !actual_revision.eq_ignore_ascii_case(expected_revision) {
            return Err(OnceError::Policy(format!(
                "flake resolved to revision {actual_revision}, expected {expected_revision}"
            )));
        }

        let installable = format!("{reference}#policy");
        let build = self.run_nix([
            "build",
            installable.as_str(),
            "--no-link",
            "--print-out-paths",
        ])?;
        if !build.success {
            return Err(self.failed(build));
        }
        let mut paths = build.stdout.lines().filter(|line| !line.trim().is_empty());
        let path = paths
            .next()
            .map(str::trim)
            .filter(|path| path.starts_with("/nix/store/"))
            .ok_or_else(|| {
                OnceError::Policy("policy build did not return one Nix store path".into())
            })?;
        if paths.next().is_some() {
            return Err(OnceError::Policy(
                "policy build returned more than one Nix store path".into(),
            ));
        }

        let policy = self.run_nix(["store", "cat", path])?;
        if !policy.success {
            return Err(self.failed(policy));
        }
        if policy.stdout.trim().is_empty() {
            return Err(OnceError::Policy("policy artifact is empty".into()));
        }
        Ok(policy.stdout)
    }

    pub fn evaluate(&self, installable: &str) -> Result<Evaluation> {
        let mut arguments = vec!["derivation", "show", installable];
        if let Some(eval_store) = &self.eval_store {
            arguments.extend(["--eval-store", eval_store]);
        }
        let output = self.run_nix(arguments)?;
        if !output.success {
            return Err(self.failed(output));
        }
        let value: serde_json::Value =
            serde_json::from_str(&output.stdout).map_err(|source| OnceError::NixJson {
                source,
                output: output.stdout.clone(),
            })?;
        let unresolved_drv = value
            .get("derivations")
            .and_then(serde_json::Value::as_object)
            .and_then(|object| object.keys().next())
            .map(|path| crate::trace::normalize_store_path(path))
            .or_else(|| {
                value
                    .as_object()
                    .and_then(|object| object.keys().find(|key| key.ends_with(".drv")))
                    .cloned()
            });
        Ok(Evaluation { unresolved_drv })
    }

    pub fn trace(&self, installable: &str) -> Result<TraceQuery> {
        let mut arguments = vec!["store", "build-trace", "info", installable, "--json"];
        if let Some(eval_store) = &self.eval_store {
            arguments.extend(["--eval-store", eval_store]);
        }
        let output = self.run_nix(arguments)?;
        if output.success {
            let records =
                TraceRecords::parse(&output.stdout).map_err(|source| OnceError::NixJson {
                    source,
                    output: output.stdout.clone(),
                })?;
            let evidence = if self
                .store
                .as_deref()
                .is_none_or(|store| store.starts_with("local?") || store == "daemon")
            {
                TrustEvidence::LocalStore
            } else {
                TrustEvidence::UnverifiedRemote
            };
            return Ok(TraceQuery::Found { records, evidence });
        }

        let diagnostic = output.stderr.trim().to_owned();
        let lower = diagnostic.to_ascii_lowercase();
        if lower.contains("unbuilt derivation")
            || lower.contains("no build trace")
            || lower.contains("does not have a realization")
        {
            Ok(TraceQuery::Miss(diagnostic))
        } else if lower.contains("untrusted")
            || lower.contains("not signed")
            || lower.contains("signature") && lower.contains("invalid")
        {
            Ok(TraceQuery::Untrusted(diagnostic))
        } else {
            Ok(TraceQuery::Failed(diagnostic))
        }
    }

    pub fn build(&self, installable: &str) -> Result<()> {
        let mut arguments = vec!["build", installable, "--no-link", "--json"];
        if let Some(eval_store) = &self.eval_store {
            arguments.extend(["--eval-store", eval_store]);
        }
        let output = self.run_nix(arguments)?;
        if output.success {
            Ok(())
        } else {
            Err(self.failed(output))
        }
    }

    fn configured_features(&self) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct Setting<T> {
            value: T,
        }

        let output = self.run_raw(["config", "show", "--json"])?;
        if !output.success {
            return Err(self.failed(output));
        }
        let settings: BTreeMap<String, Setting<serde_json::Value>> =
            serde_json::from_str(&output.stdout).map_err(|source| OnceError::NixJson {
                source,
                output: output.stdout.clone(),
            })?;
        Ok(settings
            .get("experimental-features")
            .and_then(|setting| setting.value.as_array())
            .map(|features| {
                features
                    .iter()
                    .filter_map(|feature| feature.as_str().map(ToOwned::to_owned))
                    .collect()
            })
            .unwrap_or_default())
    }

    fn run_nix<I, S>(&self, arguments: I) -> Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.executable);
        command.arg("--extra-experimental-features").arg(FEATURES);
        if let Some(store) = &self.store {
            command.arg("--store").arg(store);
        }
        self.backend.configure_nix(&mut command);
        command.args(arguments);
        self.run(command)
    }

    fn run_raw<I, S>(&self, arguments: I) -> Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = Command::new(&self.executable);
        self.backend.configure_nix(&mut command);
        command.args(arguments);
        self.run(command)
    }

    fn run(&self, mut command: Command) -> Result<CommandOutput> {
        tracing::debug!(program = %self.executable, command = ?command, "running Nix");
        let output = command.output().map_err(|source| OnceError::Command {
            program: self.executable.clone(),
            source,
        })?;
        Ok(CommandOutput {
            success: output.status.success(),
            status: output.status.code().unwrap_or(40),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    fn failed(&self, output: CommandOutput) -> OnceError {
        OnceError::CommandFailed {
            program: self.executable.clone(),
            status: output.status,
            stderr: output.stderr.trim().to_owned(),
        }
    }
}

pub fn parse_nix_version(value: &str) -> Result<Version> {
    value
        .split_whitespace()
        .rev()
        .map(|part| {
            part.trim_matches(|character: char| {
                !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '+'))
            })
        })
        .find_map(|part| {
            Version::parse(part).ok().or_else(|| {
                let (release, snapshot) = part.split_once("pre")?;
                if snapshot.is_empty() {
                    return None;
                }
                let snapshot = snapshot
                    .chars()
                    .map(|character| {
                        if character.is_ascii_alphanumeric() || character == '-' {
                            character
                        } else {
                            '-'
                        }
                    })
                    .collect::<String>();
                Version::parse(&format!("{release}-pre.{snapshot}")).ok()
            })
        })
        .ok_or_else(|| OnceError::NixVersion(value.to_owned()))
}

fn validate_policy_reference(reference: &str, expected_revision: &str) -> Result<()> {
    if expected_revision.len() != 40
        || !expected_revision
            .bytes()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(OnceError::Policy(
            "policy revision must be a full 40-character Git commit".into(),
        ));
    }

    let path = reference.strip_prefix("github:").ok_or_else(|| {
        OnceError::Policy("v0.2 requires a github:OWNER/REPO/COMMIT policy reference".into())
    })?;
    let mut segments = path.split('/');
    let owner = segments.next().unwrap_or_default();
    let repository = segments.next().unwrap_or_default();
    let revision = segments.next().unwrap_or_default();
    if owner.is_empty()
        || repository.is_empty()
        || segments.next().is_some()
        || !revision.eq_ignore_ascii_case(expected_revision)
    {
        return Err(OnceError::Policy(
            "policy reference must embed the same full commit supplied by --policy-revision".into(),
        ));
    }
    Ok(())
}

fn parse_flake_revision(output: &str) -> Result<String> {
    let metadata: serde_json::Value =
        serde_json::from_str(output).map_err(|source| OnceError::NixJson {
            source,
            output: output.to_owned(),
        })?;
    metadata
        .get("revision")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            metadata
                .pointer("/locked/rev")
                .and_then(serde_json::Value::as_str)
        })
        .map(ToOwned::to_owned)
        .ok_or_else(|| OnceError::Policy("Nix metadata did not report a locked revision".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upstream_version() {
        assert_eq!(
            parse_nix_version("nix (Nix) 2.35.0").expect("version"),
            Version::new(2, 35, 0)
        );
    }

    #[test]
    fn parses_determinate_version() {
        assert_eq!(
            parse_nix_version("nix (Determinate Nix 3.22.2) 2.35.2").expect("version"),
            Version::new(2, 35, 2)
        );
    }

    #[test]
    fn parses_upstream_snapshot_version() {
        let version =
            parse_nix_version("nix (Nix) 2.36.0pre20260822_88b09c6").expect("snapshot version");
        assert_eq!(
            version,
            Version::parse("2.36.0-pre.20260822-88b09c6").expect("normalized version")
        );
        assert!(version > Version::new(2, 35, 2));
        assert!(version < Version::new(2, 36, 0));
    }

    #[test]
    fn version_policy_boundaries() {
        let minimum = Version::new(2, 35, 0);
        assert!(Version::new(2, 34, 9) < minimum);
        assert!(Version::new(2, 35, 0) >= minimum);
        assert!(Version::new(2, 36, 0) >= minimum);
    }

    #[test]
    fn accepts_immutable_github_policy_reference() {
        let revision = "0123456789abcdef0123456789abcdef01234567";
        validate_policy_reference(
            &format!("github:closure-labs/once-policy/{revision}"),
            revision,
        )
        .expect("immutable reference");
    }

    #[test]
    fn rejects_mutable_policy_reference() {
        let revision = "0123456789abcdef0123456789abcdef01234567";
        assert!(
            validate_policy_reference("github:closure-labs/once-policy/main", revision).is_err()
        );
    }

    #[test]
    fn reads_revision_from_metadata() {
        let revision = "0123456789abcdef0123456789abcdef01234567";
        let output = format!(r#"{{"locked":{{"rev":"{revision}"}}}}"#);
        assert_eq!(parse_flake_revision(&output).expect("revision"), revision);
    }
}
