use clap::Parser;
#[tokio::main]
async fn main() {
    let cli = match bkpi::cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let text = bkpi::credentials::redact_text(&error.to_string());
            if error.use_stderr() {
                eprint!("{text}");
            } else {
                print!("{text}");
            }
            std::process::exit(error.exit_code());
        }
    };
    if let Err(error) = bkpi::cli::run(cli).await {
        eprintln!(
            "bkpi: {}",
            bkpi::credentials::redact_text(&error.to_string())
        );
        std::process::exit(1);
    }
}
