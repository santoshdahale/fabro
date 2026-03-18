use anyhow::{bail, Result};
use clap::Args;

use fabro_config::dotenv;

#[derive(Args)]
pub struct SecretGetArgs {
    /// Name of the secret
    pub key: String,
}

#[derive(Args)]
pub struct SecretListArgs {
    /// Show values alongside keys
    #[arg(long)]
    pub show_values: bool,
}

#[derive(Args)]
pub struct SecretRmArgs {
    /// Name of the secret to remove
    pub key: String,
}

#[derive(Args)]
pub struct SecretSetArgs {
    /// Name of the secret
    pub key: String,
    /// Value to store
    pub value: String,
}

pub fn get_command(args: &SecretGetArgs) -> Result<()> {
    let path = dotenv::env_file_path()?;
    match dotenv::get_env_value(&path, &args.key)? {
        Some(value) => {
            println!("{value}");
            Ok(())
        }
        None => bail!("secret not found: {}", args.key),
    }
}

pub fn list_command(args: &SecretListArgs) -> Result<()> {
    let path = dotenv::env_file_path()?;
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => bail!("failed to read {}: {e}", path.display()),
    };
    let pairs = dotenv::parse_env(&contents);
    for (key, value) in pairs {
        if args.show_values {
            println!("{key}={value}");
        } else {
            println!("{key}");
        }
    }
    Ok(())
}

pub fn rm_command(args: &SecretRmArgs) -> Result<()> {
    let path = dotenv::env_file_path()?;
    let contents = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            bail!("secret not found: {}", args.key)
        }
        Err(e) => bail!("failed to read {}: {e}", path.display()),
    };
    // Check the key actually exists before removing
    let pairs = dotenv::parse_env(&contents);
    if !pairs.iter().any(|(k, _)| k == &args.key) {
        bail!("secret not found: {}", args.key);
    }
    let updated = dotenv::remove_env_key(&contents, &args.key);
    dotenv::write_env_file(&path, &updated)?;
    eprintln!("Removed {}", args.key);
    Ok(())
}

pub fn set_command(args: &SecretSetArgs) -> Result<()> {
    let path = dotenv::env_file_path()?;
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = dotenv::merge_env(&existing, &[(&args.key, &args.value)]);
    dotenv::write_env_file(&path, &merged)?;
    eprintln!("Set {}", args.key);
    Ok(())
}
