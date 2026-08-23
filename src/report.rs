use std::{
    collections::BTreeSet,
    env,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use serde::Serialize;

use crate::{
    config::{Config, IaMode},
    error::{OnceError, Result},
    nix::{DoctorState, TraceQuery, TrustEvidence},
    trace::normalize_store_path,
    types::{Action, Decision, ResultEnvelope},
};

#[derive(Clone, Copy, Debug)]
pub enum InspectionView {
    Check,
    Resolve,
    Trace,
    Trust,
    Explain,
}

pub fn inspection(
    report: &ResultEnvelope,
    query: &TraceQuery,
    config: &Config,
    view: InspectionView,
    json: bool,
) -> Result<()> {
    match view {
        InspectionView::Check => result(report, json),
        InspectionView::Resolve => resolve(report, query, json),
        InspectionView::Trace => trace(report, query, json),
        InspectionView::Trust => trust(report, config, json),
        InspectionView::Explain if json => result(report, true),
        InspectionView::Explain => {
            human_explain(report, query);
            Ok(())
        }
    }
}

pub fn result(report: &ResultEnvelope, json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(report)?);
    } else {
        human_result(report);
    }
    Ok(())
}

fn human_result(report: &ResultEnvelope) {
    println!("Closure Labs — Once");
    println!();
    println!("installable: {}", report.installable);
    if let Some(path) = &report.derivation.unresolved {
        println!("unresolved:  {path}");
    }
    if let Some(path) = &report.derivation.resolved {
        println!("resolved:    {path}");
    }
    if let Some(output) = &report.derivation.output_name {
        println!("output:      {output}");
    }
    println!("trace:       {}", report.trace.status);
    if !report.trace.signers.is_empty() {
        println!("signer:      {}", report.trace.signers.join(", "));
    }
    println!("decision:    {}", report.decision);
    println!("action:      {}", report.action);
    for diagnostic in &report.diagnostics {
        eprintln!("note: {diagnostic}");
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ResolveReport<'a> {
    schema: &'static str,
    installable: &'a str,
    decision: Decision,
    action: Action,
    nix_version: &'a Option<String>,
    status: &'static str,
    unresolved_derivation: &'a Option<String>,
    resolved_derivation: Option<String>,
    output_name: Option<String>,
    diagnostics: &'a [String],
}

fn resolve(report: &ResultEnvelope, query: &TraceQuery, json: bool) -> Result<()> {
    let (status, resolved_derivation, output_name) = resolution(query);
    let view = ResolveReport {
        schema: "dev.closurelabs.once/resolve/v1",
        installable: &report.installable,
        decision: report.decision,
        action: report.action,
        nix_version: &report.nix_version,
        status,
        unresolved_derivation: &report.derivation.unresolved,
        resolved_derivation,
        output_name,
        diagnostics: &report.diagnostics,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!("Closure Labs — Once resolve\n");
        println!("installable: {}", view.installable);
        println!(
            "unresolved:  {}",
            optional(view.unresolved_derivation.as_deref())
        );
        println!("status:      {}", view.status);
        println!(
            "resolved:    {}",
            optional(view.resolved_derivation.as_deref())
        );
        println!("output:      {}", optional(view.output_name.as_deref()));
        print_diagnostics(view.diagnostics);
    }
    Ok(())
}

fn resolution(query: &TraceQuery) -> (&'static str, Option<String>, Option<String>) {
    match query {
        TraceQuery::Found { records, .. } => {
            records
                .entries
                .last()
                .map_or(("unknown", None, None), |entry| {
                    (
                        "resolved",
                        Some(normalize_store_path(&entry.key.drv_path)),
                        Some(entry.key.output_name.clone()),
                    )
                })
        }
        TraceQuery::Miss(_) => ("unresolved", None, None),
        TraceQuery::Untrusted(_) => ("untrusted", None, None),
        TraceQuery::Failed(_) => ("error", None, None),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceReport<'a> {
    schema: &'static str,
    installable: &'a str,
    decision: Decision,
    action: Action,
    status: &'static str,
    evidence: Option<&'static str>,
    entries: Vec<TraceEntryReport>,
    opaque_paths: Vec<String>,
    diagnostics: &'a [String],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TraceEntryReport {
    drv_path: String,
    output_name: String,
    out_path: String,
    signer_key_names: Vec<String>,
    signature_count: usize,
}

fn trace(report: &ResultEnvelope, query: &TraceQuery, json: bool) -> Result<()> {
    let (evidence, entries, opaque_paths) = trace_details(query);
    let view = TraceReport {
        schema: "dev.closurelabs.once/trace/v1",
        installable: &report.installable,
        decision: report.decision,
        action: report.action,
        status: query_status(query),
        evidence,
        entries,
        opaque_paths,
        diagnostics: &report.diagnostics,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!("Closure Labs — Once trace\n");
        println!("installable: {}", view.installable);
        println!("status:      {}", view.status);
        println!("evidence:    {}", view.evidence.unwrap_or("unknown"));
        println!("entries:     {}", view.entries.len());
        for entry in &view.entries {
            println!();
            println!("key:         {}^{}", entry.drv_path, entry.output_name);
            println!("output:      {}", entry.out_path);
            println!("signatures:  {}", entry.signature_count);
            println!(
                "signers:     {}",
                if entry.signer_key_names.is_empty() {
                    "none".into()
                } else {
                    entry.signer_key_names.join(", ")
                }
            );
        }
        print_diagnostics(view.diagnostics);
    }
    Ok(())
}

fn query_status(query: &TraceQuery) -> &'static str {
    match query {
        TraceQuery::Found { records, .. } if records.entries.is_empty() => "missing",
        TraceQuery::Found { .. } => "found",
        TraceQuery::Miss(_) => "missing",
        TraceQuery::Untrusted(_) => "untrusted",
        TraceQuery::Failed(_) => "error",
    }
}

fn trace_details(query: &TraceQuery) -> (Option<&'static str>, Vec<TraceEntryReport>, Vec<String>) {
    let TraceQuery::Found { records, evidence } = query else {
        return (None, Vec::new(), Vec::new());
    };
    let evidence = Some(match evidence {
        TrustEvidence::LocalStore => "local-store",
        TrustEvidence::UnverifiedRemote => "unverified-remote",
    });
    let entries = records
        .entries
        .iter()
        .map(|entry| {
            let signer_key_names: Vec<String> = entry
                .value
                .signatures
                .iter()
                .map(|signature| signature.key_name.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            TraceEntryReport {
                drv_path: normalize_store_path(&entry.key.drv_path),
                output_name: entry.key.output_name.clone(),
                out_path: normalize_store_path(&entry.value.out_path),
                signer_key_names,
                signature_count: entry.value.signatures.len(),
            }
        })
        .collect();
    let opaque_paths = records
        .opaque_paths
        .iter()
        .map(|path| normalize_store_path(path))
        .collect();
    (evidence, entries, opaque_paths)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrustReport<'a> {
    schema: &'static str,
    installable: &'a str,
    decision: Decision,
    action: Action,
    may_skip: bool,
    required_signatures: usize,
    accepted_key_names: &'a [String],
    ia_mode: &'static str,
    trace_status: &'a str,
    accepted_signers: &'a [String],
    accepted_signature_count: usize,
    diagnostics: &'a [String],
}

fn trust(report: &ResultEnvelope, config: &Config, json: bool) -> Result<()> {
    let view = TrustReport {
        schema: "dev.closurelabs.once/trust/v1",
        installable: &report.installable,
        decision: report.decision,
        action: report.action,
        may_skip: report.decision.may_skip(),
        required_signatures: config.trust.required_signatures,
        accepted_key_names: &config.trust.accepted_key_names,
        ia_mode: match config.trust.ia_mode {
            IaMode::Deny => "deny",
            IaMode::Warn => "warn",
            IaMode::AllowTrusted => "allow-trusted",
        },
        trace_status: &report.trace.status,
        accepted_signers: &report.trace.signers,
        accepted_signature_count: report.trace.signature_count,
        diagnostics: &report.diagnostics,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&view)?);
    } else {
        println!("Closure Labs — Once trust\n");
        println!("installable:          {}", view.installable);
        println!("trace:                {}", view.trace_status);
        println!("required signatures:  {}", view.required_signatures);
        println!("accepted signatures:  {}", view.accepted_signature_count);
        println!(
            "accepted signers:     {}",
            if view.accepted_signers.is_empty() {
                "none".into()
            } else {
                view.accepted_signers.join(", ")
            }
        );
        println!("IA mode:              {}", view.ia_mode);
        println!("decision:             {}", view.decision);
        println!("action:               {}", view.action);
        print_diagnostics(view.diagnostics);
    }
    Ok(())
}

fn human_explain(report: &ResultEnvelope, query: &TraceQuery) {
    let (resolution_status, resolved, output_name) = resolution(query);
    println!("Closure Labs — Once explain\n");
    println!("Installable:\n  {}\n", report.installable);
    println!(
        "Nix:\n  version: {}\n",
        optional(report.nix_version.as_deref())
    );
    println!(
        "Evaluation:\n  unresolved derivation: {}\n",
        optional(report.derivation.unresolved.as_deref())
    );
    println!("Resolution:\n  status: {resolution_status}");
    println!("  resolved derivation: {}", optional(resolved.as_deref()));
    println!("  output: {}\n", optional(output_name.as_deref()));
    println!("Inputs:");
    println!(
        "  content-addressed: {}",
        count(report.inputs.content_addressed)
    );
    println!(
        "  input-addressed: {}",
        count(report.inputs.input_addressed)
    );
    println!("  unknown: {}\n", count(report.inputs.unknown));
    println!("Build trace:");
    println!("  status: {}", report.trace.status);
    println!("  output: {}", optional(report.trace.out_path.as_deref()));
    println!("  accepted signatures: {}", report.trace.signature_count);
    println!(
        "  accepted signers: {}\n",
        if report.trace.signers.is_empty() {
            "none".into()
        } else {
            report.trace.signers.join(", ")
        }
    );
    println!(
        "Decision:\n  {}\n  action: {}",
        report.decision, report.action
    );
    print_diagnostics(&report.diagnostics);
}

fn optional(value: Option<&str>) -> &str {
    value.unwrap_or("unknown")
}

fn count(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".into(), |count| count.to_string())
}

fn print_diagnostics(diagnostics: &[String]) {
    for diagnostic in diagnostics {
        eprintln!("note: {diagnostic}");
    }
}

pub fn doctor(state: &DoctorState, json: bool) -> Result<()> {
    if json {
        let value = serde_json::json!({
            "schema": "dev.closurelabs.once/doctor/v1",
            "nixVersion": state.version,
            "minimumVersion": state.minimum,
            "buildTraceCommand": state.build_trace_command,
            "caDerivationsConfigured": state.ca_derivations_configured,
            "requiredFeatures": state.required_features,
            "backend": {
                "kind": state.backend.kind,
                "store": state.backend.store,
            },
            "suitable": state.suitable(),
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        println!("Closure Labs — Once doctor");
        println!();
        println!("Nix version:              {}", state.version);
        println!("Required minimum:         {}", state.minimum);
        println!(
            "build-trace command:      {}",
            yes_no(state.build_trace_command)
        );
        println!(
            "ca-derivations configured: {}",
            yes_no(state.ca_derivations_configured)
        );
        println!("Backend:                  {}", state.backend.kind);
        if let Some(store) = &state.backend.store {
            println!("Store:                    {store}");
        }
        println!("Suitable:                 {}", yes_no(state.suitable()));
    }
    Ok(())
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

pub fn github_summary(report: &ResultEnvelope, enabled: bool) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    let Ok(path) = env::var("GITHUB_STEP_SUMMARY") else {
        return Ok(());
    };
    append_summary(Path::new(&path), report)
}

fn append_summary(path: &Path, report: &ResultEnvelope) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|source| OnceError::Summary {
            path: PathBuf::from(path),
            source,
        })?;
    writeln!(file, "## Once: {}\n", report.decision).map_err(|source| OnceError::Summary {
        path: PathBuf::from(path),
        source,
    })?;
    writeln!(file, "- Installable: `{}`", report.installable).map_err(|source| {
        OnceError::Summary {
            path: PathBuf::from(path),
            source,
        }
    })?;
    writeln!(file, "- Trace: `{}`", report.trace.status).map_err(|source| OnceError::Summary {
        path: PathBuf::from(path),
        source,
    })?;
    writeln!(file, "- Action: `{}`\n", report.action).map_err(|source| OnceError::Summary {
        path: PathBuf::from(path),
        source,
    })?;
    Ok(())
}
