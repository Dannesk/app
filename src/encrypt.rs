use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use rand::RngExt;
use zeroize::Zeroize;

use crate::secure::SecureBytes;

// Borrows its inputs (`&str`) — the caller owns the secret (a locked
// `SecureString`) and is responsible for wiping it; encrypt only reads.

pub fn encrypt_data(
    passphrase: &str,
    seed: &str,
) -> Result<(String, String, String), String> {
    // Note: Use rand::thread_rng().gen() if on rand v0.8.x
    let salt: [u8; 16] = rand::rng().random();
    let iv: [u8; 12] = rand::rng().random();

    // Dereference string slice for derivation.
    match derive_key_from_passphrase(passphrase, &salt) {
        Ok(key) => {
            let cipher = Aes256Gcm::new_from_slice(key.as_bytes()).map_err(|e| e.to_string())?;

            // Seed guard
            let encrypted_data = cipher
                .encrypt(&Nonce::from(iv), seed.as_bytes())
                .map_err(|e| e.to_string())?;

            let base64_encrypted = BASE64.encode(&encrypted_data);
            let base64_salt = BASE64.encode(salt);
            let base64_iv = BASE64.encode(iv);

            // key is automatically zeroized here because of Zeroizing<Vec<u8>>
            // passphrase and seed guards both wiped on drop
            Ok((base64_encrypted, base64_salt, base64_iv))
        }
        Err(e) => Err(e),
    }
}

fn derive_key_from_passphrase(passphrase: &str, salt: &[u8]) -> Result<SecureBytes, String> {
    let mut key = [0u8; 32];

    // OWASP Recommended parameters for Argon2id (approx: 64MB RAM, 3 iterations, 4 lanes)
    let params = Params::new(65536, 3, 4, None).map_err(|e| e.to_string())?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| e.to_string())?;

    // Move the derived key into mlocked memory, then wipe the stack array.
    let result = SecureBytes::new(key.to_vec());
    key.zeroize();

    Ok(result)
}

