mod cli;
mod page;
mod plugin;
mod protocol;
mod relay;
mod transport;

use clap::{Parser, Subcommand, ValueEnum};
use std::io::IsTerminal;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "browser-cli",
    about = "Browser session CLI with Native Messaging relay"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum BrowserKind {
    Chrome,
    Firefox,
}

#[derive(Subcommand)]
enum Command {
    /// Start the native messaging relay server
    Relay,
    /// Generate the Native Messaging host manifest
    Setup {
        /// Target browser host manifest to write
        #[arg(long, value_enum, default_value_t = BrowserKind::Chrome)]
        browser: BrowserKind,
        /// Browser extension ID. If omitted, a placeholder is written.
        #[arg(long)]
        extension_id: Option<String>,
        /// Override the manifest file path instead of using the default location.
        #[arg(long, conflicts_with = "user_data_dir")]
        manifest_path: Option<PathBuf>,
        /// Write manifest into <user-data-dir>/NativeMessagingHosts/ (for portable installs with custom --user-data-dir).
        #[arg(long, conflicts_with = "manifest_path")]
        user_data_dir: Option<PathBuf>,
    },
    /// Remove the Native Messaging host manifest (and registry key on Windows)
    Teardown {
        /// Target browser to remove
        #[arg(long, value_enum, default_value_t = BrowserKind::Chrome)]
        browser: BrowserKind,
        /// Override the manifest file path instead of using the default location.
        #[arg(long, conflicts_with = "user_data_dir")]
        manifest_path: Option<PathBuf>,
        /// Remove manifest from <user-data-dir>/NativeMessagingHosts/.
        #[arg(long, conflicts_with = "manifest_path")]
        user_data_dir: Option<PathBuf>,
    },
    /// Open a URL in the browser
    Open {
        /// URL to open
        url: String,
    },
    /// Close a browser session
    Close {
        /// Session ID to close
        session_id: Option<String>,
        /// Close all sessions
        #[arg(long, conflicts_with = "session_id")]
        all: bool,
    },
    /// List open tabs
    List,
    /// Get structured page content
    Page {
        /// Session ID
        session_id: String,
        /// Page number for paginated content
        #[arg(short, long)]
        page: Option<u32>,
        /// Go to next page relative to current scroll position
        #[arg(long, conflicts_with_all = ["page", "prev"])]
        next: bool,
        /// Go to previous page relative to current scroll position
        #[arg(long, conflicts_with_all = ["page", "next"])]
        prev: bool,
        /// Bypass cache and fetch a fresh snapshot from the browser
        #[arg(long)]
        fresh: bool,
        /// Output as JSON instead of XML
        #[arg(long)]
        json: bool,
    },
    /// Click an element by ID
    Click {
        /// Session ID
        session_id: String,
        /// Element ID to click
        id: u32,
        /// Page number used to resolve element IDs
        #[arg(short, long)]
        page: Option<u32>,
        /// Open link targets in a new session instead of navigating the current one
        #[arg(long)]
        new_session: bool,
    },
    /// Type text into an input element
    Type {
        /// Session ID
        session_id: String,
        /// Element ID to type into
        id: u32,
        /// Text to type
        text: String,
        /// Page number used to resolve element IDs
        #[arg(short, long)]
        page: Option<u32>,
    },
    /// Search for text on the page
    Search {
        /// Session ID
        session_id: String,
        /// Search query
        query: String,
    },
    /// Wait for a condition
    Wait {
        /// Session ID
        session_id: String,
        /// CSS selector to wait for
        #[arg(short, long)]
        selector: Option<String>,
        /// Timeout in milliseconds
        #[arg(short, long)]
        timeout: Option<u64>,
    },
    /// Get full text content of the page
    Text {
        /// Session ID
        session_id: String,
        /// Text ID returned in page output
        text_id: String,
        /// Page number used to resolve text IDs
        #[arg(short, long)]
        page: Option<u32>,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Manage and run plugins
    Plugin {
        #[command(subcommand)]
        cmd: PluginCommand,
    },
}

#[derive(Subcommand)]
enum PluginCommand {
    /// Run a plugin by name on a session
    Run {
        /// Plugin name
        name: String,
        /// Session ID
        session_id: String,
    },
    /// List installed plugins
    List,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let cli = match Cli::try_parse_from(&args) {
        Ok(cli) => cli,
        Err(err) => {
            if should_run_as_native_host(&args) {
                relay::server::run().await?;
                return Ok(());
            }
            err.exit();
        }
    };

    match cli.command {
        Command::Relay => relay::server::run().await?,
        Command::Setup {
            browser,
            ref extension_id,
            ref manifest_path,
            ref user_data_dir,
        } => {
            let browser = match browser {
                BrowserKind::Chrome => "chrome",
                BrowserKind::Firefox => "firefox",
            };
            let resolved_path = user_data_dir
                .as_deref()
                .map(|d| {
                    d.join("NativeMessagingHosts")
                        .join("com.browser_cli.relay.json")
                })
                .or_else(|| manifest_path.clone());
            cli::commands::setup(browser, extension_id.as_deref(), resolved_path.as_deref())?
        }
        Command::Teardown {
            browser,
            ref manifest_path,
            ref user_data_dir,
        } => {
            let browser = match browser {
                BrowserKind::Chrome => "chrome",
                BrowserKind::Firefox => "firefox",
            };
            let resolved_path = user_data_dir
                .as_deref()
                .map(|d| {
                    d.join("NativeMessagingHosts")
                        .join("com.browser_cli.relay.json")
                })
                .or_else(|| manifest_path.clone());
            cli::commands::teardown(browser, resolved_path.as_deref())?
        }
        Command::Open { ref url } => cli::commands::open(url).await?,
        Command::Close {
            ref session_id,
            all,
        } => cli::commands::close(session_id.as_deref(), all).await?,
        Command::List => cli::commands::list().await?,
        Command::Page {
            ref session_id,
            page,
            next,
            prev,
            fresh,
            json,
        } => cli::commands::page(session_id, page, next, prev, fresh, json).await?,
        Command::Click {
            ref session_id,
            id,
            page,
            new_session,
        } => cli::commands::click(session_id, id, page, new_session).await?,
        Command::Type {
            ref session_id,
            id,
            ref text,
            page,
        } => cli::commands::type_text(session_id, id, text, page).await?,
        Command::Search {
            ref session_id,
            ref query,
        } => cli::commands::search(session_id, query).await?,
        Command::Wait {
            ref session_id,
            ref selector,
            timeout,
        } => cli::commands::wait(session_id, selector.as_deref(), timeout).await?,
        Command::Text {
            ref session_id,
            ref text_id,
            page,
            json,
        } => cli::commands::text(session_id, text_id, page, json).await?,
        Command::Plugin { ref cmd } => match cmd {
            PluginCommand::Run { name, session_id } => {
                cli::commands::plugin(name, session_id).await?
            }
            PluginCommand::List => cli::commands::plugin_list()?,
        },
    }

    Ok(())
}

fn should_run_as_native_host(args: &[String]) -> bool {
    if std::io::stdin().is_terminal() || std::io::stdout().is_terminal() {
        return false;
    }

    if args.len() <= 1 {
        return true;
    }

    args.iter().skip(1).any(|arg| {
        arg.starts_with("chrome-extension://")
            || arg.starts_with("moz-extension://")
            || arg.starts_with("--parent-window=")
            || arg.ends_with(".json")
            || arg.contains('@')
            || arg.chars().all(|c| c.is_ascii_digit())
    })
}

#[cfg(test)]
mod tests {
    use super::should_run_as_native_host;

    #[test]
    fn native_host_detection_matches_browser_args() {
        assert!(should_run_as_native_host(&[
            "browser-cli".into(),
            "chrome-extension://abcdefghijklmnop/".into(),
        ]));
        assert!(should_run_as_native_host(&[
            "browser-cli".into(),
            "browser-cli@browser-cli".into(),
            "/tmp/com.browser_cli.relay.json".into(),
        ]));
    }
}
