use std::{
    io::stdout,
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
    let test_command = language_impl.test_command(&cli.file, cli.line)?;
    // let test_command = tmux_wrap(test_command)?;
    let mut stdout = stdout().lock();
    serde_json::to_writer(&mut stdout, &test_command)?;

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
struct TestCommand {
    command: String,
    args: Vec<String>,
    statusline: String,
}

trait Language {
    fn test_command(&self, file: &Path, line: usize) -> Result<TestCommand>;
}

#[allow(dead_code)]
fn command<I>(command: &str, args: I) -> Result<String>
where
    I: IntoIterator<Item: AsRef<std::ffi::OsStr>>,
{
    let output = std::process::Command::new(command).args(args).output()?;
    color_eyre::eyre::ensure!(output.status.success(), "`{}` failed", command);
    Ok(String::from_utf8(output.stdout)?)
}
