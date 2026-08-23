use std::collections::{BTreeMap, BTreeSet};

use crate::{
    config::{Config, IaMode},
    nix::{Evaluation, TraceQuery, TrustEvidence},
    trace::normalize_store_path,
    types::{
        Action, Decision, DerivationSummary, ExitCode, InputSummary, ResultEnvelope, TraceSummary,
    },
};

pub struct PolicyResult {
    pub report: ResultEnvelope,
    pub exit_code: ExitCode,
}

pub fn evaluate(
    config: &Config,
    installable: &str,
    nix_version: Option<String>,
    evaluation: Evaluation,
    query: TraceQuery,
) -> PolicyResult {
    let mut report = empty_report(installable, nix_version, evaluation);
    let exit_code = match query {
        TraceQuery::Miss(diagnostic) => {
            report.decision = Decision::Miss;
            report.action = Action::Build;
            report.trace.status = "missing".into();
            report.diagnostics.push(diagnostic);
            ExitCode::Miss
        }
        TraceQuery::Untrusted(diagnostic) => {
            report.decision = Decision::Untrusted;
            report.trace.status = "untrusted".into();
            report.diagnostics.push(diagnostic);
            ExitCode::Untrusted
        }
        TraceQuery::Failed(diagnostic) => {
            report.decision = Decision::Error;
            report.trace.status = "error".into();
            report.diagnostics.push(diagnostic);
            ExitCode::Internal
        }
        TraceQuery::Found { records, evidence } => {
            classify_found(config, &mut report, records, evidence)
        }
    };
    PolicyResult { report, exit_code }
}

fn classify_found(
    config: &Config,
    report: &mut ResultEnvelope,
    records: crate::trace::TraceRecords,
    evidence: TrustEvidence,
) -> ExitCode {
    if records.entries.is_empty() {
        report.decision = Decision::Miss;
        report.action = Action::Build;
        report.trace.status = "missing".into();
        return ExitCode::Miss;
    }

    let mut outputs: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for entry in &records.entries {
        outputs
            .entry((entry.key.drv_path.clone(), entry.key.output_name.clone()))
            .or_default()
            .insert(entry.value.out_path.clone());
    }
    if outputs.values().any(|paths| paths.len() > 1) {
        report.decision = Decision::Conflict;
        report.trace.status = "conflict".into();
        report
            .diagnostics
            .push("one resolved derivation output maps to multiple output paths".into());
        return ExitCode::Conflict;
    }

    if evidence == TrustEvidence::UnverifiedRemote {
        report.decision = Decision::Unsupported;
        report.trace.status = "found-unverified".into();
        report.diagnostics.push(
            "Nix 2.35 build-trace info exposes remote signatures without validating them; remote skip is unsupported"
                .into(),
        );
        return ExitCode::Unsupported;
    }

    let entry = records.entries.last().expect("non-empty trace records");
    report.derivation.resolved = Some(normalize_store_path(&entry.key.drv_path));
    report.derivation.output_name = Some(entry.key.output_name.clone());
    report.trace.out_path = Some(normalize_store_path(&entry.value.out_path));
    let accepted_names: BTreeSet<&str> = config
        .trust
        .accepted_key_names
        .iter()
        .map(String::as_str)
        .collect();
    for base_entry in &records.entries {
        let accepted_count = base_entry
            .value
            .signatures
            .iter()
            .map(|signature| signature.key_name.as_str())
            .filter(|name| accepted_names.contains(name))
            .collect::<BTreeSet<_>>()
            .len();
        if accepted_count < config.trust.required_signatures {
            report.decision = Decision::Untrusted;
            report.trace.status = "untrusted".into();
            report.diagnostics.push(format!(
                "base trace {}^{} has {accepted_count} accepted signer(s); policy requires {}",
                base_entry.key.drv_path,
                base_entry.key.output_name,
                config.trust.required_signatures
            ));
            return ExitCode::Untrusted;
        }
    }
    let signers: BTreeSet<String> = entry
        .value
        .signatures
        .iter()
        .filter(|signature| accepted_names.contains(signature.key_name.as_str()))
        .map(|signature| signature.key_name.clone())
        .collect();
    report.trace.signers = signers.into_iter().collect();
    report.trace.signature_count = report.trace.signers.len();

    report.trace.status = "found".into();
    report.action = Action::Skip;
    match config.trust.ia_mode {
        IaMode::Deny => {
            report.decision = Decision::Unsupported;
            report.action = Action::Fail;
            report.diagnostics.push(
                "input-addressed dependency classification is unavailable; deny mode fails closed"
                    .into(),
            );
            ExitCode::IaRejected
        }
        IaMode::Warn => {
            report.decision = Decision::AcceptedWithIaTrust;
            report.diagnostics.push(
                "input-addressed dependency counts are unavailable through the Nix 2.35 CLI".into(),
            );
            ExitCode::Accepted
        }
        IaMode::AllowTrusted => {
            report.decision = Decision::Unsupported;
            report.action = Action::Fail;
            report.diagnostics.push(
                "allow-trusted IA classification is unavailable through the Nix 2.35 CLI".into(),
            );
            ExitCode::Unsupported
        }
    }
}

fn empty_report(
    installable: &str,
    nix_version: Option<String>,
    evaluation: Evaluation,
) -> ResultEnvelope {
    ResultEnvelope {
        schema: "dev.closurelabs.once/result/v1",
        installable: installable.to_owned(),
        decision: Decision::Error,
        action: Action::Fail,
        nix_version,
        derivation: DerivationSummary {
            unresolved: evaluation.unresolved_drv,
            resolved: None,
            output_name: None,
        },
        trace: TraceSummary {
            status: "unknown".into(),
            out_path: None,
            signers: Vec::new(),
            signature_count: 0,
        },
        inputs: InputSummary::default(),
        diagnostics: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use semver::Version;

    use super::*;
    use crate::{
        config::{IaMode, NixConfig, OnceConfig, ReportingConfig, TrustConfig},
        trace::{Signature, TraceEntry, TraceKey, TraceRecords, TraceValue},
    };

    fn config(mode: IaMode) -> Config {
        Config {
            schema: 1,
            once: OnceConfig {
                policy_version: "poc-v1".into(),
            },
            nix: NixConfig {
                minimum_version: Version::new(2, 35, 0),
                required_experimental_features: vec![],
            },
            trust: TrustConfig {
                required_signatures: 1,
                accepted_key_names: vec!["ci-1".into()],
                ia_mode: mode,
            },
            reporting: ReportingConfig::default(),
        }
    }

    fn found(signers: &[&str]) -> TraceQuery {
        TraceQuery::Found {
            records: TraceRecords {
                entries: vec![TraceEntry {
                    key: TraceKey {
                        drv_path: "abc-check.drv".into(),
                        output_name: "out".into(),
                    },
                    value: TraceValue {
                        out_path: "def-check".into(),
                        signatures: signers
                            .iter()
                            .map(|name| Signature {
                                key_name: (*name).into(),
                                sig: "AA==".into(),
                            })
                            .collect(),
                    },
                }],
                opaque_paths: vec![],
            },
            evidence: TrustEvidence::LocalStore,
        }
    }

    #[test]
    fn accepts_configured_signer_with_warning_mode() {
        let result = evaluate(
            &config(IaMode::Warn),
            ".#demo",
            Some("2.35.2".into()),
            Evaluation::default(),
            found(&["ci-1"]),
        );
        assert_eq!(result.report.decision, Decision::AcceptedWithIaTrust);
        assert_eq!(result.exit_code, ExitCode::Accepted);
    }

    #[test]
    fn rejects_unaccepted_signer() {
        let result = evaluate(
            &config(IaMode::Warn),
            ".#demo",
            None,
            Evaluation::default(),
            found(&["other-1"]),
        );
        assert_eq!(result.report.decision, Decision::Untrusted);
    }

    #[test]
    fn deny_mode_fails_closed_on_unknown_inputs() {
        let result = evaluate(
            &config(IaMode::Deny),
            ".#demo",
            None,
            Evaluation::default(),
            found(&["ci-1"]),
        );
        assert_eq!(result.exit_code, ExitCode::IaRejected);
    }

    #[test]
    fn allow_trusted_mode_is_explicitly_unsupported() {
        let result = evaluate(
            &config(IaMode::AllowTrusted),
            ".#demo",
            None,
            Evaluation::default(),
            found(&["ci-1"]),
        );
        assert_eq!(result.report.decision, Decision::Unsupported);
        assert_eq!(result.exit_code, ExitCode::Unsupported);
    }

    #[test]
    fn rejects_conflicting_outputs() {
        let mut query = match found(&["ci-1"]) {
            TraceQuery::Found { records, .. } => records,
            _ => unreachable!(),
        };
        let mut second = query.entries[0].clone();
        second.value.out_path = "other-check".into();
        query.entries.push(second);
        let result = evaluate(
            &config(IaMode::Warn),
            ".#demo",
            None,
            Evaluation::default(),
            TraceQuery::Found {
                records: query,
                evidence: TrustEvidence::LocalStore,
            },
        );
        assert_eq!(result.report.decision, Decision::Conflict);
        assert_eq!(result.exit_code, ExitCode::Conflict);
    }

    #[test]
    fn remote_signature_metadata_is_not_trusted() {
        let query = match found(&["ci-1"]) {
            TraceQuery::Found { records, .. } => TraceQuery::Found {
                records,
                evidence: TrustEvidence::UnverifiedRemote,
            },
            _ => unreachable!(),
        };
        let result = evaluate(
            &config(IaMode::Warn),
            ".#demo",
            None,
            Evaluation::default(),
            query,
        );
        assert_eq!(result.report.decision, Decision::Unsupported);
        assert_eq!(result.exit_code, ExitCode::Unsupported);
    }
}
