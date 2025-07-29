use std::{
    io::{IsTerminal, stdout},
    path::{Path, PathBuf},
};

use color_eyre::eyre::{Result, bail};
use serde::Serialize;

mod go_impl;
mod rust_impl;
mod tree_sitter_utils;

#[derive(clap::Parser, Debug)]
struct Cli {
    #[arg(long, short)]
    file: PathBuf,
    #[arg(long, short)]
    line: usize,
}

fn main() -> Result<()> {
    color_eyre::install()?;

    let cli = <Cli as clap::Parser>::parse();

    let language_impl = identify_language(&cli.file)?;
    let test_commands = language_impl.test_commands(&cli.file, cli.line)?;
    let mut stdout = stdout().lock();
    if stdout.is_terminal() {
        serde_json::to_writer_pretty(&mut stdout, &test_commands)?;
        println!();
    } else {
        serde_json::to_writer(&mut stdout, &test_commands)?;
    }

    Ok(())
}

fn identify_language(file: &Path) -> Result<Box<dyn Language>> {
    if let Some(ext) = file.extension() {
        if ext == "rs" {
            return Ok(Box::new(rust_impl::RustImpl));
        } else if ext == "go" {
            return Ok(Box::new(go_impl::GoImpl));
        }
    }

    bail!("unknown file type")
}

#[must_use]
#[derive(Serialize)]
struct TestCommands {
    file: TestCommand,
    file_and_line: TestCommand,
    file_debugger: Option<TestCommand>,
    file_and_line_debugger: Option<TestCommand>,
}

#[must_use]
#[derive(Serialize)]
struct TestCommand {
    command: String,
    args: Vec<String>,
}

trait Language {
    fn test_commands(&self, file: &Path, line: usize) -> Result<TestCommands>;
}
