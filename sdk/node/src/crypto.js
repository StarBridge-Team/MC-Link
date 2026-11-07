// 密码学原语：HMAC-SHA256 / HKDF-SHA256 / AES-256-GCM。
//
// 全部走 WebCrypto（`globalThis.crypto.subtle`），理由：
//
// 1. Bun 对 `node:crypto` 的覆盖是"部分实现"，而 `Crypto`/`SubtleCrypto` 是"完全实现"；
// 2. Node 22+ 也把 `crypto.subtle` 放进了全局，因此同一份代码两边都能跑；
// 3. 协议本来就是异步的，WebCrypto 只有异步 API 不构成限制。
//
// 与核心（Rust 侧 `aes-gcm` + `hkdf` + `hmac`）的字节级一致性由
// `sdk/rust/tests/vectors.rs` 的固定向量与 `src-tauri` 的端到端用例共同锁定。

import { concatBytes, fromBase64, toBase64, utf8 } from "./bytes.js";

const subtle = () => globalThis.crypto.subtle;

/** 导入 HMAC 密钥。 */
async function hmacKey(key) {
  return subtle().importKey("raw", key, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
}

/** HMAC-SHA256，返回原始摘要字节。 */
export async function hmacSha256(key, data) {
  const sig = await subtle().sign("HMAC", await hmacKey(key), data);
  return new Uint8Array(sig);
}

/** HMAC-SHA256，返回小写 hex（握手 `proof` 用的就是它）。 */
export async function hmacSha256Hex(key, data) {
  const digest = await hmacSha256(key, data);
  return Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * HKDF-SHA256 派生。
 *
 * `infoParts` 是**多段 info 的拼接**——与 Rust 侧 `expand_multi_info(&[a, b])`
 * 等价。这一点最容易写错：把两段当成嵌套结构或漏掉 transcript，派生出的密钥
 * 与核心对不上，表现是"握手成功但第一帧就解密失败"。
 */
export async function hkdfSha256(ikm, salt, infoParts, length = 32) {
  const base = await subtle().importKey("raw", ikm, "HKDF", false, ["deriveBits"]);
  const bits = await subtle().deriveBits(
    {
      name: "HKDF",
      hash: "SHA-256",
      salt,
      info: concatBytes(...infoParts.map((p) => (typeof p === "string" ? utf8(p) : p))),
    },
    base,
    length * 8,
  );
  return new Uint8Array(bits);
}

/** AES-256-GCM 加密，返回 `{ nonce, sealed }`，`sealed` = 密文 || 16 字节 authTag。 */
export async function aesGcmSeal(key, aad, plaintext) {
  const nonce = globalThis.crypto.getRandomValues(new Uint8Array(12));
  const cryptoKey = await subtle().importKey("raw", key, "AES-GCM", false, ["encrypt"]);
  const sealed = await subtle().encrypt(
    { name: "AES-GCM", iv: nonce, additionalData: aad, tagLength: 128 },
    cryptoKey,
    plaintext,
  );
  return { nonce, sealed: new Uint8Array(sealed) };
}

/** AES-256-GCM 解密；`sealed` 末尾 16 字节是 authTag（WebCrypto 也吃这个布局）。 */
export async function aesGcmOpen(key, aad, nonce, sealed) {
  const cryptoKey = await subtle().importKey("raw", key, "AES-GCM", false, ["decrypt"]);
  const plain = await subtle().decrypt(
    { name: "AES-GCM", iv: nonce, additionalData: aad, tagLength: 128 },
    cryptoKey,
    sealed,
  );
  return new Uint8Array(plain);
}

// ---------------------------------------------------------------------------
// 信封（SecureFrame）
// ---------------------------------------------------------------------------

/**
 * 把 `Message` 封装成线上信封。
 *
 * AAD 必须与核心逐字节一致：`v1|{sid}|{seq}`。
 */
export async function sealMessage(key, sid, seq, message) {
  const plaintext = utf8(JSON.stringify(message));
  const aad = utf8(`v1|${sid}|${seq}`);
  const { nonce, sealed } = await aesGcmSeal(key, aad, plaintext);
  return {
    v: 1,
    sid,
    seq,
    n: toBase64(nonce),
    ct: toBase64(sealed),
  };
}

/** 解开线上信封，返回 `Message`。 */
export async function openMessage(key, sid, frame) {
  if (frame.v !== 1) throw new Error(`不支持的协议版本: ${frame.v}`);
  if (frame.sid !== sid) throw new Error("会话 ID 不匹配");
  const aad = utf8(`v1|${sid}|${frame.seq}`);
  const plain = await aesGcmOpen(key, aad, fromBase64(frame.n), fromBase64(frame.ct));
  return JSON.parse(new TextDecoder().decode(plain));
}
