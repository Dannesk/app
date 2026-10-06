use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use zeroize::Zeroize;

use crate::secure::{SecureBytes, SecureString};

pub fn decrypt_data(
    passphrase: &str,
    encrypted_base64: &str,
    salt_base64: &str,
    iv_base64: &str,
) -> Result<SecureString, String> {
    let encrypted_data = BASE64.decode(encrypted_base64).map_err(|e| e.to_string())?;
    let salt = BASE64.decode(salt_base64).map_err(|e| e.to_string())?;
    let iv = BASE64.decode(iv_base64).map_err(|e| e.to_string())?;

    match derive_key_from_passphrase(passphrase, &salt) {
        Ok(key) => {
            let cipher = Aes256Gcm::new_from_slice(key.as_bytes()).map_err(|e| e.to_string())?;
            let nonce = Nonce::try_from(&iv[..]).map_err(|_| "invalid IV length".to_string())?;

            let decrypted_bytes = cipher
                .decrypt(&nonce, encrypted_data.as_ref())
                .map_err(|e| e.to_string())?;

            // `from_utf8` reuses the same allocation (no copy); `SecureString::new`
            // then locks that buffer and zeroizes it on drop.
            let decrypted_string = String::from_utf8(decrypted_bytes).map_err(|e| e.to_string())?;

            Ok(SecureString::new(decrypted_string))
        }
        Err(e) => Err(e),
    }
}

fn derive_key_from_passphrase(passphrase: &str, salt: &[u8]) -> Result<SecureBytes, String> {
    let mut key = [0u8; 32];

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
