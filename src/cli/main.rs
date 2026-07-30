use clap::Parser;
use vllm_doctor::cli::args::Args;
use vllm_doctor::cli::commands;

#[tokio::main]
async fn main() {
    if let Err(code) = commands::run(Args::parse()).await {
        std::process::exit(code);
    }
}
