use anyhow::Result;
use std::path::Path;

use catalyst_crypto::{PrivateKey, PublicKey};

pub fn load_or_generate_private_key(path: &Path, auto_generate: bool) -> Result<PrivateKey> {
    if path.exists() {
        let s = std::fs::read_to_string(path)?;
        let s = s.trim();
        let s = s.strip_prefix("0x").unwrap_or(s);
        let bytes = hex::decode(s)?;
        anyhow::ensure!(bytes.len() == 32, "private key must be 32 bytes hex");
        let mut b = [0u8; 32];
        b.copy_from_slice(&bytes);
        return Ok(PrivateKey::from_bytes(b));
    }

    anyhow::ensure!(auto_generate, "key file {:?} missing and auto-generate disabled", path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let mut rng = rand::rngs::OsRng;
    let pk = PrivateKey::generate(&mut rng);
    save_private_key_hex(path, &pk)?;
    Ok(pk)
}

pub fn save_private_key_hex(path: &Path, key: &PrivateKey) -> Result<()> {
    let s = hex::encode(key.to_bytes());
    std::fs::write(path, format!("{s}\n"))?;
    Ok(())
}

pub fn public_key_bytes(private_key: &PrivateKey) -> [u8; 32] {
    private_key.public_key().to_bytes()
}

pub fn public_key_from_bytes(bytes: [u8; 32]) -> Result<PublicKey> {
    Ok(PublicKey::from_bytes(bytes)?)
}

