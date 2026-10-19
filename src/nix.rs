use std::{
    cell::OnceCell,
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    path::Path,
    process::{Command, Stdio},
};

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

#[derive(Clone, Debug)]
pub struct Evaluation {
    pub unresolved_drv: Option<String>,
    pub output_name: String,
    pub system: String,
    pub policy_version: Option<String>,
    pub content_addressed: bool,
}

impl Default for Evaluation {
    fn default() -> Self {
        Self {
            unresolved_drv: None,
            output_name: "out".into(),
            system: String::new(),
            policy_version: None,
            content_addressed: false,
        }
    }
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
    VerifiedNative,
}

#[derive(Clone, Debug)]
pub struct DoctorState {
    pub version: Version,
    pub minimum: Version,
    pub build_trace_command: bool,
    pub ca_derivations_configured: bool,
    pub required_features: Vec<String>,
    pub required_features_configured: bool,
    pub backend: BackendDescription,
}

impl DoctorState {
    #[must_use]
    pub fn suitable(&self) -> bool {
        self.version >= self.minimum
            && self.build_trace_command
            && self.ca_derivations_configured
            && self.required_features_configured
    }
}

pub struct NixAdapter {
    executable: String,
    backend: EnvironmentBackend,
    store: Option<String>,
    eval_store: Option<String>,
    resolved_store: OnceCell<String>,
}

impl Default for NixAdapter {
    fn default() -> Self {
        Self {
            executable: std::env::var("ONCE_NIX_BIN").unwrap_or_else(|_| "nix".into()),
            backend: EnvironmentBackend,
            store: std::env::var("ONCE_NIX_STORE")
                .or_else(|_| std::env::var("NIX_REMOTE"))
                .ok(),
            eval_store: std::env::var("ONCE_NIX_EVAL_STORE").ok(),
            resolved_store: OnceCell::new(),
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
            required_features_configured: config
                .nix
                .required_experimental_features
                .iter()
                .all(|feature| configured.contains(feature)),
            backend: BackendDescription {
                store: Some(self.effective_store()?),
                ..self.backend.describe()
            },
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

    /// Evaluate one static derivation and select one explicit output, without IFD.
    pub fn evaluate(&self, installable: &str) -> Result<Evaluation> {
        let value = self.derivations(installable, false)?;
        let derivations = derivation_map(&value)?;
        if derivations.len() != 1 {
            return Err(OnceError::InvalidConfig(
                "Once requires exactly one target derivation".into(),
            ));
        }
        let (path, derivation) = derivations.iter().next().expect("one derivation");
        validate_static_derivation(derivation)?;
        let outputs = derivation
            .get("outputs")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| OnceError::InvalidConfig("target derivation has no outputs".into()))?;
        let selected = installable.rsplit_once('^').map(|(_, output)| output);
        let output_name = match selected {
            Some(output)
                if output != "*" && !output.contains(',') && outputs.contains_key(output) =>
            {
                output.to_owned()
            }
            Some(_) => {
                return Err(OnceError::InvalidConfig(
                    "select exactly one existing target output".into(),
                ));
            }
            None if outputs.len() == 1 => outputs.keys().next().expect("one output").clone(),
            None => {
                return Err(OnceError::InvalidConfig(
                    "multi-output targets require an explicit ^output selection".into(),
                ));
            }
        };
        let output = &outputs[&output_name];
        Ok(Evaluation {
            unresolved_drv: Some(crate::trace::normalize_store_path(path)),
            output_name,
            system: derivation
                .get("system")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            policy_version: derivation
                .pointer("/env/policyVersion")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            content_addressed: is_floating_ca(output),
        })
    }

    /// Expose the static graph to bundle export without realizing any outputs.
    pub fn recursive_derivations(&self, installable: &str) -> Result<serde_json::Value> {
        self.derivations(installable, true)
    }

    fn derivations(&self, installable: &str, recursive: bool) -> Result<serde_json::Value> {
        let mut arguments = vec!["derivation", "show", installable];
        if recursive {
            arguments.push("--recursive");
        }
        arguments.extend(read_only_arguments());
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
                output: output.stdout,
            })?;
        for derivation in derivation_map(&value)?.values() {
            validate_static_derivation(derivation)?;
        }
        Ok(value)
    }

    /// Query only a cache whose native claims the caller has already verified.
    /// The returned evidence remains remote; callers must retain that verification evidence.
    pub fn trace_cached(&self, installable: &str, cache_path: &Path) -> Result<TraceQuery> {
        let eval_store = self.eval_store.clone().unwrap_or(self.effective_store()?);
        if !is_local_store(&eval_store) {
            return Err(OnceError::InvalidConfig(
                "verified-cache resolution requires an explicit local evaluation store".into(),
            ));
        }
        let evaluation = self.evaluate(installable)?;
        let unresolved = evaluation.unresolved_drv.as_deref().ok_or_else(|| {
            OnceError::InvalidConfig("target evaluation did not identify a derivation".into())
        })?;
        let target = format!("{unresolved}^{}", evaluation.output_name);
        let cache = cache_path
            .canonicalize()
            .map_err(|source| OnceError::ReadConfig {
                path: cache_path.into(),
                source,
            })?;
        let cache_store = format!("file://{}", cache.display());
        // Native build-trace info reads derivations from its operational store,
        // not the evaluation store, during its second resolution pass. Seed only
        // the freshly evaluated .drv objects; never copy their source/output closure.
        // Native cache upload enforces reference availability, so transport these
        // evaluator-authored objects with native path-info and dump-path instead.
        let graph = self.recursive_derivations(installable)?;
        self.stage_derivations(&eval_store, &cache, &graph)?;
        let mut arguments = vec![
            "store",
            "build-trace",
            "info",
            &target,
            "--json",
            "--eval-store",
            &eval_store,
            "--option",
            "substituters",
            "",
            "--option",
            "extra-substituters",
            "",
            "--option",
            "accept-flake-config",
            "false",
        ];
        arguments.extend(read_only_arguments());
        let output = self.run_nix_at(&cache_store, arguments)?;
        let query = self.parse_trace_output(output, TrustEvidence::UnverifiedRemote)?;
        if let TraceQuery::Found {
            mut records,
            evidence,
        } = query
        {
            // Nix emits the requested root output as an opaquePath, and emits
            // queried dependency claims alongside its root claim. Match that
            // native root path explicitly rather than relying on record ordering.
            let target_paths: std::collections::BTreeSet<_> = records
                .opaque_paths
                .iter()
                .map(|path| crate::trace::normalize_store_path(path))
                .collect();
            if target_paths.len() != 1 {
                return Ok(TraceQuery::Failed(
                    "cached query did not identify one target output path".into(),
                ));
            }
            records.entries.retain(|entry| {
                entry.key.output_name == evaluation.output_name
                    && target_paths
                        .contains(&crate::trace::normalize_store_path(&entry.value.out_path))
            });
            if records.entries.is_empty() {
                return Ok(TraceQuery::Miss(
                    "cache has no trace for the selected target output".into(),
                ));
            }
            let identities: std::collections::BTreeSet<_> = records
                .entries
                .iter()
                .map(|entry| &entry.key.drv_path)
                .collect();
            if identities.len() != 1 {
                return Ok(TraceQuery::Failed(
                    "cached query returned ambiguous target identities".into(),
                ));
            }
            Ok(TraceQuery::Found { records, evidence })
        } else {
            Ok(query)
        }
    }

    fn stage_derivations(
        &self,
        eval_store: &str,
        cache: &Path,
        graph: &serde_json::Value,
    ) -> Result<()> {
        let paths: Vec<String> = derivation_map(graph)?
            .keys()
            .map(|path| crate::trace::normalize_store_path(path))
            .collect();
        if paths.is_empty() {
            return Err(OnceError::Bundle(
                "evaluated derivation graph is empty".into(),
            ));
        }
        let mut infos = serde_json::Map::new();
        // Bound argv size for large evaluated graphs (without a shell or stdin protocol).
        for batch in paths.chunks(128) {
            let mut arguments = vec!["path-info", "--json", "--json-format", "1"];
            arguments.extend(batch.iter().map(String::as_str));
            let output = self.run_nix_at(eval_store, arguments)?;
            if !output.success {
                return Err(self.failed(output));
            }
            let value: serde_json::Value =
                serde_json::from_str(&output.stdout).map_err(|source| OnceError::NixJson {
                    source,
                    output: output.stdout,
                })?;
            let map = value
                .as_object()
                .ok_or_else(|| OnceError::Bundle("invalid derivation path metadata".into()))?;
            infos.extend(map.clone());
        }
        fs::create_dir_all(cache.join("nar"))
            .map_err(|error| OnceError::Bundle(error.to_string()))?;
        for path in &paths {
            let info = infos.get(path).ok_or_else(|| {
                OnceError::Bundle(format!("missing evaluated derivation metadata: {path}"))
            })?;
            let nar_hash = info
                .get("narHash")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| OnceError::Bundle("missing derivation NAR hash".into()))?;
            let nar_size = info
                .get("narSize")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| OnceError::Bundle("missing derivation NAR size".into()))?;
            let basename = crate::verification::basename(path).map_err(OnceError::Bundle)?;
            let url = format!("nar/{}.nar", &basename[..32]);
            let file = fs::File::create(cache.join(&url))
                .map_err(|error| OnceError::Bundle(error.to_string()))?;
            let mut command = self.nix_command_at(eval_store);
            command.args(["store", "dump-path", path]);
            command.stdout(Stdio::from(file));
            let dumped = self.run(command)?;
            if !dumped.success {
                return Err(self.failed(dumped));
            }
            if fs::metadata(cache.join(&url))
                .map_err(|error| OnceError::Bundle(error.to_string()))?
                .len()
                != nar_size
            {
                return Err(OnceError::Bundle(
                    "evaluated derivation NAR size changed during staging".into(),
                ));
            }
            let references = info
                .get("references")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| OnceError::Bundle("missing derivation references".into()))?
                .iter()
                .map(|value| {
                    let path = value
                        .as_str()
                        .ok_or_else(|| OnceError::Bundle("invalid derivation reference".into()))?;
                    crate::verification::basename(path)
                        .map(str::to_owned)
                        .map_err(OnceError::Bundle)
                })
                .collect::<Result<Vec<_>>>()?
                .join(" ");
            // This is transport metadata for current evaluator-authored .drv
            // objects, never an alternative derivation resolver or build claim.
            let mut narinfo = format!(
                "StorePath: {path}\nURL: {url}\nCompression: none\nFileHash: {nar_hash}\nFileSize: {nar_size}\nNarHash: {nar_hash}\nNarSize: {nar_size}\nReferences: {references}\n"
            );
            if let Some(ca) = info.get("ca").and_then(serde_json::Value::as_str) {
                narinfo.push_str(&format!("CA: {ca}\n"));
            }
            fs::write(cache.join(format!("{}.narinfo", &basename[..32])), narinfo)
                .map_err(|error| OnceError::Bundle(error.to_string()))?;
        }
        Ok(())
    }

    pub fn trace(&self, installable: &str) -> Result<TraceQuery> {
        let mut arguments = vec!["store", "build-trace", "info", installable, "--json"];
        arguments.extend(read_only_arguments());
        if let Some(eval_store) = &self.eval_store {
            arguments.extend(["--eval-store", eval_store]);
        }
        let store = self.effective_store()?;
        let output = self.run_nix_at(&store, arguments)?;
        self.parse_trace_output(
            output,
            if is_local_store(&store) {
                TrustEvidence::LocalStore
            } else {
                TrustEvidence::UnverifiedRemote
            },
        )
    }

    fn parse_trace_output(
        &self,
        output: CommandOutput,
        evidence: TrustEvidence,
    ) -> Result<TraceQuery> {
        if output.success {
            let records =
                TraceRecords::parse(&output.stdout).map_err(|source| OnceError::NixJson {
                    source,
                    output: output.stdout.clone(),
                })?;
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
        self.run_nix_at(&self.effective_store()?, arguments)
    }

    fn run_nix_at<I, S>(&self, store: &str, arguments: I) -> Result<CommandOutput>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.nix_command_at(store);
        command.args(arguments);
        self.run(command)
    }

    fn nix_command_at(&self, store: &str) -> Command {
        let mut command = Command::new(&self.executable);
        command.arg("--extra-experimental-features").arg(FEATURES);
        command.arg("--store").arg(store);
        self.backend.configure_nix(&mut command);
        command
    }

    fn effective_store(&self) -> Result<String> {
        if let Some(store) = self.resolved_store.get() {
            return Ok(store.clone());
        }
        let configured = if let Some(store) = &self.store {
            store.clone()
        } else {
            let output = self.run_raw(["config", "show", "--json"])?;
            if !output.success {
                return Err(self.failed(output));
            }
            let value: serde_json::Value =
                serde_json::from_str(&output.stdout).map_err(|source| OnceError::NixJson {
                    source,
                    output: output.stdout,
                })?;
            value
                .pointer("/store/value")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("auto")
                .to_owned()
        };
        let store = if configured == "auto" {
            if Path::new("/nix/var/nix/daemon-socket/socket").exists() {
                "daemon"
            } else {
                "local"
            }
            .to_owned()
        } else if configured.trim().is_empty() {
            return Err(OnceError::InvalidConfig(
                "Nix store must not be empty".into(),
            ));
        } else {
            configured
        };
        let _ = self.resolved_store.set(store.clone());
        Ok(store)
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

fn read_only_arguments() -> [&'static str; 5] {
    [
        "--option",
        "allow-import-from-derivation",
        "false",
        "--no-update-lock-file",
        "--no-write-lock-file",
    ]
}

fn is_local_store(store: &str) -> bool {
    store == "local" || store == "daemon" || store.starts_with("local?")
}

fn derivation_map(
    value: &serde_json::Value,
) -> Result<&serde_json::Map<String, serde_json::Value>> {
    let map = value
        .get("derivations")
        .unwrap_or(value)
        .as_object()
        .ok_or_else(|| OnceError::InvalidConfig("Nix did not return a derivation map".into()))?;
    if map.keys().any(|path| !path.ends_with(".drv")) {
        return Err(OnceError::InvalidConfig(
            "invalid derivation identity returned by Nix".into(),
        ));
    }
    Ok(map)
}

fn is_floating_ca(output: &serde_json::Value) -> bool {
    output.get("hashAlgo").is_some() && output.get("hash").is_none() && output.get("path").is_none()
}

fn validate_static_derivation(derivation: &serde_json::Value) -> Result<()> {
    if derivation
        .pointer("/env/__impure")
        .and_then(serde_json::Value::as_str)
        == Some("1")
    {
        return Err(OnceError::InvalidConfig(
            "impure derivations are unsupported".into(),
        ));
    }
    // Dynamic derivations add nested dynamicDrvDeps; only static input-output dependencies are supported.
    if derivation
        .get("dynamicDrvDeps")
        .is_some_and(|value| value.as_object().is_none_or(|map| !map.is_empty()))
    {
        return Err(OnceError::InvalidConfig(
            "dynamic derivations are unsupported".into(),
        ));
    }
    for inputs in [
        derivation.get("inputDrvs"),
        derivation.pointer("/inputs/drvs"),
    ]
    .into_iter()
    .flatten()
    {
        if let Some(map) = inputs.as_object() {
            for input in map.values() {
                if input
                    .get("dynamicOutputs")
                    .is_some_and(|value| value.as_object().is_none_or(|map| !map.is_empty()))
                {
                    return Err(OnceError::InvalidConfig(
                        "dynamic derivation outputs are unsupported".into(),
                    ));
                }
            }
        }
    }
    Ok(())
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
