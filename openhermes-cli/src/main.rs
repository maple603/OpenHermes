//! CLI interface for OpenHermes Agent.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing::info;

#[derive(Parser)]
#[command(name = "hermes")]
#[command(about = "The self-improving AI agent", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive CLI chat
    Chat,
    /// Configure model
    Model {
        /// Provider:model (e.g., anthropic/claude-opus-4)
        model_spec: Option<String>,
    },
    /// Configure tools
    Tools,
    /// Gateway management
    Gateway {
        /// Gateway action (start, stop, status)
        action: String,
    },
    /// Run setup wizard
    Setup,
    /// Diagnose issues
    Doctor,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Chat) => {
            run_chat().await?;
        }
        Some(Commands::Model { model_spec }) => {
            run_model(model_spec).await?;
        }
        Some(Commands::Tools) => {
            info!("Tools configuration not yet implemented");
        }
        Some(Commands::Gateway { action }) => {
            info!("Gateway action '{}' not yet implemented", action);
        }
        Some(Commands::Setup) => {
            info!("Setup wizard not yet implemented");
        }
        Some(Commands::Doctor) => {
            run_doctor().await?;
        }
        None => {
            // Default to chat mode
            run_chat().await?;
        }
    }

    Ok(())
}

async fn run_chat() -> Result<()> {
    info!("Starting interactive chat mode...");

    // Load configuration
    openhermes_config::load_dotenv()?;
    let config = openhermes_config::load_config()?;

    // Create agent
    let agent = openhermes_core::AIAgent::from_config(&config).await?;

    info!("Agent initialized. Type your messages (Ctrl+C to exit).");

    // Simple REPL loop
    let stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    let mut reader = tokio::io::BufReader::new(stdin);
    let mut line = String::new();

    loop {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncWriteExt;

        stdout.write_all(b"\n> ").await?;
        stdout.flush().await?;

        line.clear();
        let bytes_read = reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            break; // EOF
        }

        let message = line.trim();
        if message.is_empty() {
            continue;
        }

        // Handle slash commands
        if message.starts_with('/') {
            handle_command(message).await?;
        } else {
            // Send message to agent
            match agent.chat(message).await {
                Ok(response) => {
                    stdout.write_all(response.as_bytes()).await?;
                    stdout.write_all(b"\n").await?;
                }
                Err(e) => {
                    eprintln!("Error: {}", e);
                }
            }
            stdout.flush().await?;
        }
    }

    Ok(())
}

async fn handle_command(command: &str) -> Result<()> {
    let parts: Vec<&str> = command.split_whitespace().collect();

    match parts[0] {
        "/new" | "/reset" => {
            println!("Conversation cleared");
        }
        "/help" => {
            println!("Available commands:");
            println!("  /new, /reset  - Start fresh conversation");
            println!("  /help         - Show this help");
            println!("  /quit, /exit  - Exit the CLI");
        }
        "/quit" | "/exit" => {
            std::process::exit(0);
        }
        _ => {
            println!("Unknown command: {}", parts[0]);
        }
    }

    Ok(())
}

async fn run_model(model_spec: Option<String>) -> Result<()> {
    match model_spec {
        Some(spec) => {
            info!("Switching to model: {}", spec);
            // TODO: Update config with new model
            println!("Model switching not yet implemented");
        }
        None => {
            info!("Showing current model configuration");
            let config = openhermes_config::load_config()?;
            println!("Current model: {}", config.agent.model);
        }
    }

    Ok(())
}

async fn run_doctor() -> Result<()> {
    println!("Hermes Agent Diagnostics");
    println!("========================");

    // Check config directory
    let hermes_home = openhermes_constants::get_hermes_home();
    println!("HERMES_HOME: {}", hermes_home.display());

    if hermes_home.exists() {
        println!("✓ Config directory exists");
    } else {
        println!("✗ Config directory not found");
    }

    // Check config file
    let config_path = hermes_home.join("config.yaml");
    if config_path.exists() {
        println!("✓ Config file exists");
    } else {
        println!("✗ Config file not found (using defaults)");
    }

    // Check environment variables
    let has_api_key = std::env::var("OPENAI_API_KEY").is_ok()
        || std::env::var("ANTHROPIC_API_KEY").is_ok()
        || std::env::var("OPENROUTER_API_KEY").is_ok();

    if has_api_key {
        println!("✓ API key found");
    } else {
        println!("✗ No API key found (set OPENAI_API_KEY, ANTHROPIC_API_KEY, or OPENROUTER_API_KEY)");
    }

    println!("\nDone");
    Ok(())
}
