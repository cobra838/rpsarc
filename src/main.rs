#![feature(file_buffered, read_array, iter_collect_into, try_blocks)]

mod bin_format;
mod cmd_create;
mod cmd_extract;
mod mani_json;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
  #[command(subcommand)]
  command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
  /// List archive file
  L {
    /// Input PSAR file
    in_file: PathBuf,
  },
  /// Extract archive, creating a manifest json for recreating
  X {
    /// Input PSAR file
    in_file: PathBuf,
    /// Output directory
    out_dir: PathBuf,
  },
  /// Create archive from a manifest json
  C {
    /// Manifest json file
    in_json: PathBuf,
    /// Output PSAR file
    out_file: PathBuf,
  },
}

fn main() -> Result<()> {
  match Cli::parse().command {
    Cmd::L { in_file } => cmd_extract::extract(in_file, PathBuf::new(), true),
    Cmd::X { in_file, out_dir } => cmd_extract::extract(in_file, out_dir, false),
    Cmd::C { in_json, out_file } => cmd_create::create(in_json, out_file),
  }
}
