use anyhow::Result;
use clap::Parser;

mod cli;
mod db;
mod model;
mod prompts;
mod tui;

fn main() -> Result<()> {
    let args = cli::Cli::parse();
    cli::dispatch(args)
}
