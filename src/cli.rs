use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use crate::{
    bundle,
    config::Config,
    error::Result,
    nix::NixAdapter,
    policy,
    report::{self, InspectionView},
    types::{Action, Decision, ExitCode},
};

#[derive(Debug, Parser)]
#[command(name = "once", version, about = "Build once. Recognize it thereafter.")]
pub struct Cli {
    /// Path to a local Once policy configuration.
    #[arg(long, global = true, conflicts_with = "policy_flake")]
    config: Option<PathBuf>,

    /// Immutable GitHub flake reference that exports packages.<system>.policy.
    #[arg(
        long,
        global = true,
        requires = "policy_revision",
        conflicts_with = "config"
    )]
    policy_flake: Option<String>,

    /// Expected full Git commit for --policy-flake.
    #[arg(long, global = true, requires = "policy_flake")]
    policy_revision: Option<String>,

    /// Emit a stable JSON result envelope.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Diagnose Nix and policy prerequisites.
    Doctor,
    /// Evaluate whether work may be skipped.
    Check {
        installable: String,
        /// Directory containing a native signed trace bundle.
        #[arg(long)]
        trace_bundle: Option<PathBuf>,
    },
    /// Export native signed claims without exporting output artifacts.
    Export {
        installable: String,
        #[arg(long)]
        output: PathBuf,
    },
    /// Print the stable artifact lookup name for a portable check.
    BundleKey { installable: String },
    /// Show derivation resolution state without building.
    Resolve { installable: String },
    /// Show the native build trace without building.
    Trace { installable: String },
    /// Evaluate build-trace trust policy without building.
    Trust { installable: String },
    /// Show the complete Once decision without building.
    Explain { installable: String },
    /// Check, build on a miss, and check again.
    Run { installable: String },
}

pub fn execute(cli: Cli) -> Result<i32> {
    let nix = NixAdapter::default();
    let config = match (&cli.policy_flake, &cli.policy_revision) {
        (Some(flake), Some(revision)) => {
            let source = nix.load_policy(flake, revision)?;
            Config::parse(&source, &format!("{flake}#policy"))?
        }
        (None, None) => Config::load(
            cli.config
                .as_deref()
                .unwrap_or_else(|| std::path::Path::new(".once.toml")),
        )?,
        _ => unreachable!("clap enforces external policy arguments"),
    };
    match cli.command {
        Commands::Doctor => {
            let state = nix.doctor(&config)?;
            let suitable = state.suitable();
            report::doctor(&state, cli.json)?;
            Ok(if suitable {
                ExitCode::Accepted as i32
            } else {
                ExitCode::Unsupported as i32
            })
        }
        Commands::Run { installable } => run(&nix, &config, &installable, cli.json),
        Commands::Check {
            installable,
            trace_bundle,
        } => inspect(
            &nix,
            &config,
            &installable,
            InspectionView::Check,
            cli.json,
            trace_bundle.as_deref(),
        ),
        Commands::Export {
            installable,
            output,
        } => {
            ensure_portable_version(&nix, &config)?;
            bundle::export(&nix, &config, &installable, &output)?;
            Ok(ExitCode::Accepted as i32)
        }
        Commands::BundleKey { installable } => {
            ensure_portable_version(&nix, &config)?;
            println!(
                "{}",
                bundle::artifact_key(&nix.evaluate(&installable)?, &config)?
            );
            Ok(ExitCode::Accepted as i32)
        }
        Commands::Resolve { installable } => inspect(
            &nix,
            &config,
            &installable,
            InspectionView::Resolve,
            cli.json,
            None,
        ),
        Commands::Trace { installable } => inspect(
            &nix,
            &config,
            &installable,
            InspectionView::Trace,
            cli.json,
            None,
        ),
        Commands::Trust { installable } => inspect(
            &nix,
            &config,
            &installable,
            InspectionView::Trust,
            cli.json,
            None,
        ),
        Commands::Explain { installable } => inspect(
            &nix,
            &config,
            &installable,
            InspectionView::Explain,
            cli.json,
            None,
        ),
    }
}

fn inspect(
    nix: &NixAdapter,
    config: &Config,
    installable: &str,
    view: InspectionView,
    json: bool,
    trace_bundle: Option<&Path>,
) -> Result<i32> {
    let version = nix.version()?;
    if version < config.nix.minimum_version {
        let evaluation = Default::default();
        let query = crate::nix::TraceQuery::Failed(format!(
            "Nix {version} is older than required {}",
            config.nix.minimum_version
        ));
        let mut result = policy::evaluate(
            config,
            installable,
            Some(version.to_string()),
            evaluation,
            query.clone(),
        );
        result.report.decision = Decision::Unsupported;
        result.exit_code = ExitCode::Unsupported;
        report::inspection(&result.report, &query, config, view, json)?;
        return Ok(result.exit_code as i32);
    }

    let evaluation = nix.evaluate(installable)?;
    let (query, bundle_id) = if let Some(path) = trace_bundle {
        ensure_portable_version(nix, config)?;
        match bundle::check(nix, config, installable, &evaluation, path) {
            Ok(result) => result,
            Err(crate::error::OnceError::Verification(message)) => {
                (crate::nix::TraceQuery::Untrusted(message), None)
            }
            Err(error) => return Err(error),
        }
    } else {
        (nix.trace(installable)?, None)
    };
    let mut result = policy::evaluate(
        config,
        installable,
        Some(version.to_string()),
        evaluation,
        query.clone(),
    );
    result.report.trace.bundle_id = bundle_id;
    report::inspection(&result.report, &query, config, view, json)?;
    report::github_summary(&result.report, config.reporting.github_summary)?;
    Ok(result.exit_code as i32)
}

fn run(nix: &NixAdapter, config: &Config, installable: &str, json: bool) -> Result<i32> {
    let version = nix.version()?;
    if version < config.nix.minimum_version {
        let mut result = policy::evaluate(
            config,
            installable,
            Some(version.to_string()),
            Default::default(),
            crate::nix::TraceQuery::Failed(format!(
                "Nix {version} is older than required {}",
                config.nix.minimum_version
            )),
        );
        result.report.decision = Decision::Unsupported;
        result.exit_code = ExitCode::Unsupported;
        report::result(&result.report, json)?;
        return Ok(result.exit_code as i32);
    }
    let evaluation = nix.evaluate(installable)?;
    let query = nix.trace(installable)?;
    let mut result = policy::evaluate(
        config,
        installable,
        Some(version.to_string()),
        evaluation,
        query,
    );

    if result.report.decision == Decision::Miss {
        nix.build(installable)?;
        let evaluation = nix.evaluate(installable)?;
        let query = nix.trace(installable)?;
        result = policy::evaluate(
            config,
            installable,
            Some(version.to_string()),
            evaluation,
            query,
        );
        if result.report.decision.may_skip() {
            result.report.action = Action::Built;
        }
    }

    report::result(&result.report, json)?;
    report::github_summary(&result.report, config.reporting.github_summary)?;
    Ok(result.exit_code as i32)
}

fn ensure_portable_version(nix: &NixAdapter, config: &Config) -> Result<()> {
    let version = nix.version()?;
    if version < config.nix.minimum_version || version != semver::Version::new(2, 35, 3) {
        return Err(crate::error::OnceError::Unsupported(format!(
            "portable trace format is verified only with Nix 2.35.3 (found {version})"
        )));
    }
    Ok(())
}
