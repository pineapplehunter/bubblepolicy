use clap::{Parser, Subcommand};
use color_eyre::Result;

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
    },
    /// Review traced paths and toggle allow/deny
    Scan {
        /// Paths to scan (default: current directory)
        #[arg(default_value = ".")]
        paths: Vec<String>,
    },
    /// Create a bubblewrap wrapper from a policy file
    Create {
        /// Policy file to use
        #[arg(short, long)]
        policy: Option<String>,
        /// Output file (default: stdout)
        #[arg(short, long)]
        output: Option<String>,
        /// Binary to wrap
        binary: Option<String>,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Trace { cmd } => {
            if cmd.is_empty() {
                color_eyre::bail!("Error: command required. Usage: myjail trace -- <command>");
            }
            myjail::trace::run(&cmd)?;
        }
        Commands::Scan { paths } => {
            myjail::scan::run(&paths)?;
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
