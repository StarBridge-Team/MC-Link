//! AES-256-GCM 加密/解密（带 Nonce 随机数，防重放）
//!
//! 加密输出格式: [12字节 Nonce][AEAD 密文(含16字节认证标签)]
//! 解密时从密文中提取前12字节作为 Nonce，剩余部分为密文+标签。
//! 每次加密生成随机 Nonce，相同明文每次结果不同，天然防重放。

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use rand_core::RngCore;
use sha2::{Digest, Sha256};

/// 加密后输出的 Nonce 长度（字节）
pub const NONCE_SIZE: usize = 12;
/// GCM 认证标签长度
pub const TAG_SIZE: usize = 16;

fn derive_key(password: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hasher.finalize().into()
}

/// 使用密码加密数据（AES-256-GCM，随机 Nonce）
///
/// 返回格式: `nonce(12) || ciphertext_with_tag`
/// - 每次调用生成不同的随机 Nonce，相同数据每次结果不同
/// - AEAD 自动附加 16 字节认证标签到密文末尾，防篡改
pub fn encrypt(data: &[u8], password: &str) -> Vec<u8> {
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("有效密钥");
    let mut nonce_bytes = [0u8; NONCE_SIZE];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, data).expect("加密失败");

    let mut result = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&ciphertext);
    result
}

/// 使用密码解密数据（AES-256-GCM）
///
/// 输入格式: `nonce(12) || ciphertext_with_tag`
/// 解密失败（密码错误/数据篡改）返回 None
pub fn decrypt(data: &[u8], password: &str) -> Option<Vec<u8>> {
    if data.len() < NONCE_SIZE + TAG_SIZE {
        return None;
    }
    let key = derive_key(password);
    let cipher = Aes256Gcm::new_from_slice(&key).expect("有效密钥");
    let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, ciphertext).ok()
}