mod backend;
mod cli;
mod config;
mod error;
mod nix;
mod policy;
mod report;
mod trace;
mod types;

use clap::Parser;
use cli::Cli;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let code = match cli::execute(cli) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("once: {error}");
            match error {
                error::OnceError::ReadConfig { .. }
                | error::OnceError::ParseConfig { .. }
                | error::OnceError::InvalidConfig(_)
                | error::OnceError::Policy(_) => types::ExitCode::MalformedConfig as i32,
                _ => types::ExitCode::Internal as i32,
            }
        }
    };
    std::process::exit(code);
}
