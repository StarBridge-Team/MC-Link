// 字节与文本互转（hex / base64 / UTF-8）。
//
// 不依赖 `Buffer`：同一份代码要同时跑在 Node 与 Bun 上，而 `Buffer` 是 Node 专有
// 全局（Bun 虽有兼容层，但没必要把可移植性押在兼容层上）。这里只用
// `TextEncoder` / `TextDecoder` 与手写的 base64 表——两者都是 WHATWG 标准。

const HEX = "0123456789abcdef";
const B64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/** 字节 → 小写 hex。 */
export function toHex(bytes) {
  let out = "";
  for (const b of bytes) out += HEX[b >> 4] + HEX[b & 15];
  return out;
}

/** hex → 字节。非法输入抛错（不做"尽量猜"）。 */
export function fromHex(text) {
  const clean = String(text).trim();
  if (clean.length % 2 !== 0) throw new Error("hex 长度必须是偶数");
  const out = new Uint8Array(clean.length / 2);
  for (let i = 0; i < out.length; i++) {
    const pair = clean.slice(i * 2, i * 2 + 2);
    if (!/^[0-9a-fA-F]{2}$/.test(pair)) throw new Error(`非法的 hex 字节: ${pair}`);
    out[i] = parseInt(pair, 16);
  }
  return out;
}

/** 字节 → 标准 base64（带 `=` 填充）。 */
export function toBase64(bytes) {
  let out = "";
  for (let i = 0; i < bytes.length; i += 3) {
    const a = bytes[i];
    const b = bytes[i + 1];
    const c = bytes[i + 2];
    out += B64[a >> 2];
    out += B64[((a & 3) << 4) | ((b ?? 0) >> 4)];
    out += b === undefined ? "=" : B64[((b & 15) << 2) | ((c ?? 0) >> 6)];
    out += c === undefined ? "=" : B64[c & 63];
  }
  return out;
}

/** 标准 base64 → 字节。 */
export function fromBase64(text) {
  const clean = String(text).replace(/[\r\n\s]/g, "").replace(/=+$/, "");
  const out = new Uint8Array(Math.floor((clean.length * 3) / 4));
  let acc = 0;
  let bits = 0;
  let i = 0;
  for (const ch of clean) {
    const v = B64.indexOf(ch);
    if (v < 0) throw new Error(`非法的 base64 字符: ${ch}`);
    acc = (acc << 6) | v;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out[i++] = (acc >> bits) & 0xff;
    }
  }
  return out.subarray(0, i);
}

/** 字符串 → UTF-8 字节（ASCII 快路径；协议里绝大多数拼接都是 ASCII）。 */
export function utf8(text) {
  return new TextEncoder().encode(text);
}

/** UTF-8 字节 → 字符串。 */
export function fromUtf8(bytes) {
  return new TextDecoder().decode(bytes);
}

/** 拼接若干字节数组。 */
export function concatBytes(...parts) {
  const total = parts.reduce((n, p) => n + p.length, 0);
  const out = new Uint8Array(total);
  let offset = 0;
  for (const p of parts) {
    out.set(p, offset);
    offset += p.length;
  }
  return out;
}

/** 密码学安全随机字节。 */
export function randomBytes(n) {
  const out = new Uint8Array(n);
  globalThis.crypto.getRandomValues(out);
  return out;
}

/** 随机 hex（`n` 为字节数，输出 2n 个字符）。 */
export function randomHex(n) {
  return toHex(randomBytes(n));
}
