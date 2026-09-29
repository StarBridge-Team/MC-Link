use std::path::Path;
use sha2::{Digest, Sha256};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

const BUFFER_SIZE: usize = 8192;

/// 计算文件的 SHA-256 哈希值。
pub async fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .await
        .map_err(|e| format!("打开文件失败: {}", e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; BUFFER_SIZE];

    loop {
        let n = file
            .read(&mut buf)
            .await
            .map_err(|e| format!("读取文件失败: {}", e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// 校验文件 SHA-256 是否与期望值一致。
pub async fn verify_file(path: &Path, expected: &str) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }
    let actual = sha256_file(path).await?;
    Ok(actual.eq_ignore_ascii_case(expected.trim()))
}
