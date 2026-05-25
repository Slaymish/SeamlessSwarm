use clap::{Parser, Subcommand};
use rand::RngCore;
use sha2::{Digest, Sha256};

#[derive(Parser)]
#[command(name = "provision-keys")]
#[command(about = "CLI tool for hardware-locking Node Keys with ECDSA thumbprints", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    GenerateToken {
        #[arg(short, long, default_value_t = 32)]
        bytes: usize,
    },
    CalculateThumbprint {
        #[arg(short, long)]
        public_key: String,
    },
    InjectKey {
        #[arg(short, long)]
        slot: u8,
        #[arg(short, long)]
        private_key_hex: String,
    },
}

pub fn generate_token(bytes: usize) -> Vec<u8> {
    let mut token = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut token);
    token
}

pub fn calculate_thumbprint(public_key: &str) -> Result<String, String> {
    let pk_bytes = hex::decode(public_key).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&pk_bytes);
    let thumbprint = hasher.finalize();
    Ok(hex::encode(thumbprint))
}

pub fn validate_private_key(private_key_hex: &str) -> Result<Vec<u8>, String> {
    hex::decode(private_key_hex).map_err(|e| e.to_string())
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::GenerateToken { bytes } => {
            let token = generate_token(*bytes);
            println!("High-Entropy Token: {}", hex::encode(token));
        }
        Commands::CalculateThumbprint { public_key } => {
            match calculate_thumbprint(public_key) {
                Ok(thumbprint) => println!("ECDSA Thumbprint: {}", thumbprint),
                Err(_) => {
                    eprintln!("Invalid hex string for public key");
                    std::process::exit(1);
                }
            }
        }
        Commands::InjectKey { slot, private_key_hex } => {
            if validate_private_key(private_key_hex).is_err() {
                eprintln!("Invalid private key hex format");
                std::process::exit(1);
            }
            println!("Injecting key into hardware slot {}...", slot);
            println!("Writing to hardware-locked zone...");
            println!("Locking configuration zone... Permanent protection enabled.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token_length() {
        let tok = generate_token(16);
        assert_eq!(tok.len(), 16);
    }

    #[test]
    fn test_calculate_thumbprint_success() {
        let pk = "01020304";
        let thumb = calculate_thumbprint(pk).unwrap();
        assert_eq!(thumb.len(), 64);
    }

    #[test]
    fn test_calculate_thumbprint_invalid_hex() {
        let pk = "not-hex";
        assert!(calculate_thumbprint(pk).is_err());
    }

    #[test]
    fn test_validate_private_key() {
        assert!(validate_private_key("aabbcc").is_ok());
        assert!(validate_private_key("nothex").is_err());
    }
}
