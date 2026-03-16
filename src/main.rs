use clap::{Parser, Subcommand};
use color_eyre::{eyre::bail, Result};

#[derive(Parser)]
#[command(name = "myjail")]
#[command(about = "Bubblewrap sandbox policy tool - trace, review, create", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Trace system calls and file access of a command
    Trace {
        /// Command to trace (with arguments)
        #[arg(trailing_var_arg = true)]
        cmd: Vec<String>,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Review traced paths and toggle allow/deny
    Review {
        /// Paths to review (default: current directory)
        #[arg(default_value = ".")]
        paths: Vec<String>,
        /// Generate policy without TUI
        #[arg(short, long)]
        generate_policy: bool,
        /// Output file (required)
        #[arg(short, long, required = true)]
        output: String,
    },
    /// Create a bubblewrap wrapper from a policy file
    Create {
        /// Policy file to use
        #[arg(short, long)]
        policy: Option<String>,
        /// Binary to wrap (default: /bin/sh)
        binary: Option<String>,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<String>,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Trace { cmd, output } => {
            if cmd.is_empty() {
                bail!("Error: command required. Usage: myjail trace -- <command>");
            }
            myjail::trace::run(&cmd, output.as_deref())?;
        }
        Commands::Review {
            paths,
            generate_policy,
            output,
        } => {
            myjail::review::run(&paths, generate_policy, &output)?;
        }
        Commands::Create {
            policy,
            output,
            binary,
        } => {
            myjail::create::run(policy.as_deref(), output.as_deref(), binary.as_deref())?;
        }
    }

    Ok(())
}
