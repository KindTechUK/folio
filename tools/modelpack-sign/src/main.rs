//! `modelpack-sign` — vendor-side producer CLI for signed model packs.
//!
//! Thin CLI shell: all signing/packing logic lives in `lib.rs`.

use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use modelpack_sign::{keygen, pack, verify_pack, PackMeta};

#[derive(Parser)]
#[command(
    name = "modelpack-sign",
    about = "Vendor-side producer: create and verify signed model packs."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate a fresh ed25519 keypair (PKCS8 private PEM + SPKI public PEM).
    Keygen {
        /// Directory to write private.pkcs8.pem + public.spki.pem into.
        #[arg(long)]
        out_dir: PathBuf,
    },
    /// Zip a model dir, hash, sign, and write a signed pack (self-verifies).
    Pack {
        #[arg(long)]
        model_dir: PathBuf,
        #[arg(long)]
        out: PathBuf,
        /// PKCS8 private-key PEM file (the vendor secret).
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        id: String,
        #[arg(long)]
        version: String,
        #[arg(long)]
        engine: String,
        #[arg(long)]
        engine_min_version: String,
        #[arg(long, value_enum)]
        task: Task,
        #[arg(long)]
        license: String,
        #[arg(long)]
        attribution: String,
    },
    /// Verify an existing pack against an SPKI public PEM.
    Verify {
        #[arg(long)]
        pack: PathBuf,
        /// SPKI public-key PEM file.
        #[arg(long)]
        pubkey: PathBuf,
    },
}

#[derive(Clone, ValueEnum)]
enum Task {
    Asr,
    Diarization,
}

impl Task {
    fn as_str(&self) -> &'static str {
        match self {
            Task::Asr => "asr",
            Task::Diarization => "diarization",
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Keygen { out_dir } => {
            let (private_path, public_path) = keygen(&out_dir)?;
            println!("wrote {}", private_path.display());
            println!("wrote {}", public_path.display());
            println!();
            println!(
                "NOTE: {} is the VENDOR SECRET — never commit it.",
                private_path.display()
            );
            println!(
                "      {} is the public key the firm build injects via VENDOR_PUBKEY_SPKI_PEM.",
                public_path.display()
            );
        }
        Command::Pack {
            model_dir,
            out,
            key,
            id,
            version,
            engine,
            engine_min_version,
            task,
            license,
            attribution,
        } => {
            let private_pem = std::fs::read_to_string(&key)?;
            let meta = PackMeta {
                id,
                version,
                engine,
                engine_min_version,
                task: task.as_str().to_string(),
                license,
                attribution,
            };
            pack(&model_dir, &out, &private_pem, &meta)?;
            println!("OK: wrote signed pack to {}", out.display());
            println!("    (self-verified with modelpack::verify_model_pack)");
        }
        Command::Verify { pack, pubkey } => {
            let public_pem = std::fs::read_to_string(&pubkey)?;
            match verify_pack(&pack, &public_pem) {
                Ok(attribution) => {
                    println!("OK: pack verified. attribution: {attribution}");
                }
                Err(e) => {
                    eprintln!("REJECTED: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
    Ok(())
}
