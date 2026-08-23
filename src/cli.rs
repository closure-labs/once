use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::{
    config::Config,
    error::Result,
    nix::NixAdapter,
    policy, report,
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
    Check { installable: String },
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
        Commands::Check { installable }
        | Commands::Resolve { installable }
        | Commands::Trace { installable }
        | Commands::Trust { installable }
        | Commands::Explain { installable } => inspect(&nix, &config, &installable, cli.json),
    }
}

fn inspect(nix: &NixAdapter, config: &Config, installable: &str, json: bool) -> Result<i32> {
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
            query,
        );
        result.report.decision = Decision::Unsupported;
        result.exit_code = ExitCode::Unsupported;
        report::result(&result.report, json)?;
        return Ok(result.exit_code as i32);
    }

    let evaluation = nix.evaluate(installable)?;
    let query = nix.trace(installable)?;
    let result = policy::evaluate(
        config,
        installable,
        Some(version.to_string()),
        evaluation,
        query,
    );
    report::result(&result.report, json)?;
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
