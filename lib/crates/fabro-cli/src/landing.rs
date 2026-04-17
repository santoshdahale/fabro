//! Curated landing output printed when `fabro` is invoked with no subcommand.
//!
//! `fabro --help` and `fabro help` still surface clap's full reference.

#![expect(
    clippy::print_stdout,
    reason = "curated top-level help is written directly to stdout, mirroring clap's --help output"
)]

use console::style;

pub(crate) fn print() {
    let cmd_width = "sandbox preview".len();

    println!(
        "{} AI-powered workflow orchestration.",
        style("fabro —").bold()
    );
    println!();
    println!(
        "{} {} {}",
        style("Usage:").bold().underlined(),
        style("fabro").bold(),
        style("<command>").dim()
    );
    println!();

    section(
        "Set up",
        &[
            (
                "install",
                "Set up the Fabro environment (LLMs, certs, GitHub)",
            ),
            ("doctor", "Check environment and integration health"),
            ("repo init", "Initialize Fabro in a repository"),
        ],
        cmd_width,
    );

    section(
        "Run workflows",
        &[
            ("validate", "Validate a workflow"),
            ("preflight", "Validate run configuration without executing"),
            ("run", "Launch a workflow run"),
            ("logs", "View the event log of a workflow run"),
        ],
        cmd_width,
    );

    section(
        "Server & secrets",
        &[
            ("server start", "Start the Fabro API server"),
            ("secret set", "Store a server-owned secret"),
            ("secret list", "List server-owned secrets"),
        ],
        cmd_width,
    );

    section(
        "Inspect sandboxes",
        &[
            ("sandbox ssh", "SSH into a run's sandbox"),
            (
                "sandbox preview",
                "Get a preview URL for a port on a run's sandbox",
            ),
            ("sandbox cp", "Copy files to/from a run's sandbox"),
        ],
        cmd_width,
    );

    println!("{}", style("If you need help along the way:").bold());
    println!();
    println!(
        "  Run {} for the full command reference.",
        style("fabro help").cyan().bold()
    );
    println!(
        "  Run {} for details on a specific command.",
        style("fabro <command> --help").cyan().bold()
    );
    println!(
        "  Visit {} for docs and examples.",
        style("https://docs.fabro.sh").cyan().bold()
    );
}

fn section(heading: &str, rows: &[(&str, &str)], cmd_width: usize) {
    println!("{}", style(heading).bold());
    println!();
    for (cmd, desc) in rows {
        let padded = format!("fabro {cmd:<cmd_width$}");
        println!("  {}   {}", style(padded).cyan().bold(), style(desc).dim());
    }
    println!();
}
