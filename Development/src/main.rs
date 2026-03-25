mod commands;

use clap::{Parser, Subcommand};

/// OpenPeripheral Developer CLI — build, validate, and publish addons and firmware.
#[derive(Parser)]
#[command(name = "op-dev", version, about)]
struct Cli {
    /// Server URL (default: http://127.0.0.1:8088)
    #[arg(long, env = "OP_SERVER", default_value = "http://127.0.0.1:8088")]
    server: String,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Validate an addon manifest (addon.yaml)
    Validate {
        /// Path to the addon project directory
        #[arg(default_value = ".")]
        path: String,
    },

    /// Build an addon into a distributable .opx package
    Build {
        /// Path to the addon project directory
        #[arg(default_value = ".")]
        path: String,
        /// Output directory for the .opx package
        #[arg(short, long, default_value = "./dist")]
        output: String,
    },

    /// Publish an addon package to the server
    PublishAddon {
        /// Path to the .opx package file
        package: String,
        /// Path to the addon manifest (addon.yaml)
        #[arg(short, long, default_value = "./addon.yaml")]
        manifest: String,
    },

    /// Publish firmware to the server
    PublishFirmware {
        /// Path to the firmware binary
        binary: String,
        /// Path to firmware metadata JSON
        metadata: String,
    },

    /// Publish an app update to the server
    PublishUpdate {
        /// Path to the update archive (.zip)
        archive: String,
        /// Version string (e.g. "0.2.0")
        version: String,
        /// Release notes
        #[arg(short, long)]
        notes: Option<String>,
    },

    /// Scaffold a new addon project
    Init {
        /// Addon ID (e.g. "my-brand-devices")
        name: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let cli = Cli::parse();

    let Some(command) = cli.command else {
        // No subcommand — print help and wait so the window doesn't vanish.
        use clap::CommandFactory;
        Cli::command().print_help()?;
        println!("\n\nPress Enter to exit...");
        let _ = std::io::stdin().read_line(&mut String::new());
        return Ok(());
    };

    match command {
        Commands::Validate { path } => commands::validate(&path)?,
        Commands::Build { path, output } => commands::build(&path, &output)?,
        Commands::PublishAddon { package, manifest } => {
            commands::publish_addon(&cli.server, &package, &manifest).await?
        }
        Commands::PublishFirmware { binary, metadata } => {
            commands::publish_firmware(&cli.server, &binary, &metadata).await?
        }
        Commands::PublishUpdate {
            archive,
            version,
            notes,
        } => commands::publish_update(&cli.server, &archive, &version, notes.as_deref()).await?,
        Commands::Init { name } => commands::init_addon(&name)?,
    }

    Ok(())
}
