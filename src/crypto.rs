use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Key, Nonce
};
use rand::{Rng, thread_rng};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

pub fn encrypt(plain_text: &str, key_bytes: &[u8; 32]) -> Result<String, Box<dyn std::error::Error>> {
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    
    // Generate a random 12-byte nonce
    let mut nonce_bytes = [0u8; 12];
    thread_rng().fill(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    // Encrypt
    let ciphertext = cipher.encrypt(nonce, plain_text.as_bytes())
        .map_err(|e| format!("Encryption failure: {:?}", e))?;
        
    // Combine nonce + ciphertext
    let mut combined = Vec::new();
    combined.extend_from_slice(&nonce_bytes);
    combined.extend_from_slice(&ciphertext);
    
    Ok(BASE64.encode(combined))
}

pub fn decrypt(cipher_text_base64: &str, key_bytes: &[u8; 32]) -> Result<String, Box<dyn std::error::Error>> {
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    
    let decoded = BASE64.decode(cipher_text_base64)?;
    if decoded.len() < 12 {
        return Err("Ciphertext too short".into());
    }
    
    let (nonce_bytes, ciphertext) = decoded.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    
    let decrypted_bytes = cipher.decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption failure: {:?}", e))?;
        
    let decrypted_str = String::from_utf8(decrypted_bytes)?;
    Ok(decrypted_str)
}
