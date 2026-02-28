use clap::Parser;
use arc_util::terminal::Styles;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    let styles: &'static Styles = Box::leak(Box::new(Styles::detect_stderr()));
    let cli = arc_attractor::cli::Cli::parse();

    let result = match cli.command {
        arc_attractor::cli::Command::Run(args) => arc_attractor::cli::run::run_command(args, styles).await,
        arc_attractor::cli::Command::Validate(args) => {
            arc_attractor::cli::validate::validate_command(&args, styles)
        }
        #[cfg(feature = "server")]
        arc_attractor::cli::Command::Serve(args) => {
            arc_attractor::cli::serve::serve_command(args, styles).await
        }
    };

    if let Err(e) = result {
        eprintln!(
            "{red}Error:{reset} {e:#}",
            red = styles.red, reset = styles.reset,
        );
        std::process::exit(1);
    }
}
