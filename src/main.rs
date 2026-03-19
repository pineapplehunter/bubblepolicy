use clap::{Parser, Subcommand};
use color_eyre::{eyre::bail, Result};

#[derive(Parser)]
#[command(name = "bubblepolicy")]
#[command(about = "Bubblewrap sandbox policy tool - trace, review, optimise, create", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Trace system calls and file access of a command
    Trace {
        /// Output file (default: stdout)
        #[arg(default_value = "-")]
        output: String,
        /// Command to trace (with arguments)
        #[arg(last = true)]
        cmd: Vec<String>,
    },
    /// Review traced paths in TUI and toggle allow/deny
    #[cfg(feature = "ui")]
    #[command(name = "review-ui")]
    ReviewUi {
        /// Input/output file (required)
        #[arg(required = true)]
        file: String,
    },
    /// Manipulate tree attributes via CLI
    Review {
        /// Input/output file (required)
        #[arg(required = true)]
        file: String,
        /// Set paths as read-only
        #[arg(short = 'r')]
        ro: Vec<String>,
        /// Set paths as read-write
        #[arg(short = 'w')]
        rw: Vec<String>,
        /// Set paths as tmpfs
        #[arg(short = 't')]
        tmp: Vec<String>,
        /// Set paths as deny
        #[arg(short = 'd')]
        deny: Vec<String>,
    },
    /// Optimise/dedup the policy tree (in place)
    Optimise {
        /// Input/output file (required)
        #[arg(required = true)]
        file: String,
    },
    /// Create a bubblewrap wrapper from a policy file
    Create {
        /// Policy file (default: policy.json)
        #[arg(default_value = "policy.json")]
        policy: String,
        /// Binary to wrap (default: /bin/sh)
        #[arg(default_value = "/bin/sh")]
        binary: String,
    },
}

fn main() -> Result<()> {
    color_eyre::install()?;
    let cli = Cli::parse();

    match cli.command {
        Commands::Trace { output, cmd } => {
            if cmd.is_empty() {
                bail!("Error: command required. Usage: bubblepolicy trace <output> -- <command>");
            }
            let output = if output == "-" {
                None
            } else {
                Some(output.as_str())
            };
            bubblepolicy::trace::run(&cmd, output)?;
        }
        #[cfg(feature = "ui")]
        Commands::ReviewUi { file } => {
            bubblepolicy::review_ui::run(&file)?;
        }
        Commands::Review {
            file,
            ro,
            rw,
            tmp,
            deny,
        } => {
            bubblepolicy::review::run(&file, &ro, &rw, &tmp, &deny)?;
        }
        Commands::Optimise { file } => {
            bubblepolicy::optimise::run(&file)?;
        }
        Commands::Create { policy, binary } => {
            bubblepolicy::create::run(&policy, &binary)?;
        }
    }

    Ok(())
}
