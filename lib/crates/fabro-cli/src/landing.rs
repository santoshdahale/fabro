//! Curated landing output printed when `fabro` is invoked with no subcommand.
//!
//! `fabro --help` and `fabro help` still surface clap's full reference.

#![expect(
    clippy::print_stdout,
    reason = "curated top-level help is written directly to stdout, mirroring clap's --help output"
)]

use console::style;

pub(crate) fn print() {
    let cmd_width = "server start".len();

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
            ("server start", "Start the Fabro API server"),
            ("secret set", "Store a server-owned secret"),
            ("docs", "Open the docs website in the browser"),
        ],
        cmd_width,
    );

    section(
        "Run workflows",
        &[
            ("validate", "Validate a workflow"),
            ("preflight", "Validate run configuration without executing"),
            ("run", "Launch a workflow run"),
        ],
        cmd_width,
    );

    section(
        "Inspect runs",
        &[
            ("logs", "View the event log of a workflow run"),
            ("sandbox ssh", "SSH into a run's sandbox"),
        ],
        cmd_width,
    );

    println!("{}", style("If you need help along the way:").bold());
    println!();
    println!(
        "  Run {} for more information about a command.",
        style("fabro <command> --help").cyan().bold()
    );
    println!(
        "  Join our Discord at {} to get help from the Fabro community.",
        style("https://fabro.sh/discord").cyan().bold()
    );
    println!();
    println!(
        "{}{}{}",
        style("For a full list of commands, run `").dim(),
        style("fabro help").cyan().bold(),
        style("`.").dim()
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
