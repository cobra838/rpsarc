#![feature(file_buffered, read_array, try_blocks)]

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
  /// Inspect header and likely writer profile
  I {
    /// Input PSAR file
    in_file: PathBuf,
  },
  /// Export a JSON recipe without extracting files
  J {
    /// Override auto-detection for the JSON recipe: ps3 or orbis_ps4
    #[arg(long)]
    profile: Option<String>,
    /// Input PSAR file
    in_file: PathBuf,
    /// Output JSON file
    out_json: PathBuf,
  },
  /// Extract archive, creating a manifest json for recreating
  X {
    /// Override auto-detection for the JSON recipe: ps3 or orbis_ps4
    #[arg(long)]
    profile: Option<String>,
    /// Extract only a path, directory, or extension glob (repeatable)
    #[arg(long, value_name = "PATH_OR_GLOB")]
    include: Vec<String>,
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
    Cmd::L { in_file } => cmd_extract::extract(in_file, PathBuf::new(), true, None, vec![]),
    Cmd::I { in_file } => cmd_extract::inspect(in_file),
    Cmd::J { profile, in_file, out_json } => cmd_extract::export_json(in_file, out_json, profile),
    Cmd::X { profile, include, in_file, out_dir } => cmd_extract::extract(in_file, out_dir, false, profile, include),
    Cmd::C { in_json, out_file } => cmd_create::create(in_json, out_file),
  }
}
