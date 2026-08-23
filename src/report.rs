use std::{
    env,
    fs::OpenOptions,
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    error::{OnceError, Result},
    nix::DoctorState,
    types::ResultEnvelope,
};

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
