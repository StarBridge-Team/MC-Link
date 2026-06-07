//! AES-256 加密/解密（ECB 模式 + PKCS7 padding）

use aes::Aes256;
use cipher::{BlockDecrypt, BlockEncrypt, KeyInit};
use cipher::generic_array::GenericArray;
use sha2::{Digest, Sha256};

fn derive_key(password: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}

/// 使用密码加密数据
pub fn encrypt(data: &[u8], password: &str) -> Vec<u8> {
    let key = derive_key(password);
    let cipher = Aes256::new(GenericArray::from_slice(&key));
    let block_size = 16;
    let padding_len = block_size - (data.len() % block_size);
    let mut padded = data.to_vec();
    padded.extend(vec![padding_len as u8; padding_len]);
    let mut encrypted = Vec::new();
    for chunk in padded.chunks_exact(block_size) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.encrypt_block(&mut block);
        encrypted.extend_from_slice(&block);
    }
    encrypted
}

/// 使用密码解密数据
pub fn decrypt(data: &[u8], password: &str) -> Option<Vec<u8>> {
    if data.len() % 16 != 0 {
        return None;
    }
    let key = derive_key(password);
    let cipher = Aes256::new(GenericArray::from_slice(&key));
    let mut decrypted = Vec::new();
    for chunk in data.chunks_exact(16) {
        let mut block = GenericArray::clone_from_slice(chunk);
        cipher.decrypt_block(&mut block);
        decrypted.extend_from_slice(&block);
    }
    if let Some(&padding_len) = decrypted.last() {
        let padding_len = padding_len as usize;
        if padding_len > 0 && padding_len <= 16 {
            decrypted.truncate(decrypted.len() - padding_len);
            return Some(decrypted);
        }
    }
    None
}