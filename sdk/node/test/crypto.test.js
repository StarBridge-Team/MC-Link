// SDK 自测：用 `node:crypto` 作为**独立参考实现**校验 WebCrypto 路径。
//
// 参考实现的意义：如果两边都错成同一个样子，端到端测试可能仍然通过（因为
// 插件与核心用的都是我们自己的代码）——但 HKDF/HMAC 是标准算法，和
// `node:crypto` 对齐就能证明我们不是"自洽但错误"。
//
// 运行：`node --test sdk/node/test`

import assert from "node:assert/strict";
import { createHmac, hkdfSync, randomBytes as nodeRandom } from "node:crypto";
import { test } from "node:test";

import {
  concatBytes,
  fromBase64,
  fromHex,
  toBase64,
  toHex,
  utf8,
} from "../src/bytes.js";
import { aesGcmSeal, aesGcmOpen, hkdfSha256, hmacSha256Hex } from "../src/crypto.js";
import { openMessage, sealMessage } from "../src/crypto.js";
import { transcript } from "../src/protocol.js";

test("hex 与 base64 往返", () => {
  const bytes = new Uint8Array([0, 1, 2, 127, 128, 254, 255]);
  assert.equal(toHex(bytes), "0001027f80feff");
  assert.deepEqual(Array.from(fromHex("0001027f80feff")), Array.from(bytes));

  assert.equal(toBase64(new Uint8Array([72, 105])), "SGk=");
  assert.deepEqual(Array.from(fromBase64("SGk=")), [72, 105]);
  // 空输入与无填充输入
  assert.equal(toBase64(new Uint8Array([])), "");
  assert.deepEqual(Array.from(fromBase64("")), []);
  assert.deepEqual(Array.from(fromBase64("SGk")), [72, 105]);
});

test("非法 hex 必须报错而不是猜", () => {
  assert.throws(() => fromHex("abc"));
  assert.throws(() => fromHex("zz"));
});

test("HMAC-SHA256 与 node:crypto 一致", async () => {
  const key = nodeRandom(32);
  const msg = utf8("mclink-plugin-auth-v1|sid|dev.a.a|s|p|plugin");
  const mine = await hmacSha256Hex(new Uint8Array(key), msg);
  const ref = createHmac("sha256", key).update(msg).digest("hex");
  assert.equal(mine, ref);
});

test("HKDF-SHA256 与 node:crypto 一致（含多段 info 拼接）", async () => {
  const ikm = nodeRandom(32);
  const salt = utf8("aabbccdd");
  const t = transcript("sid", "dev.a.a", "aa", "bb", "session");

  for (const label of ["mclink-plugin-c2s-v1", "mclink-plugin-s2c-v1"]) {
    const mine = await hkdfSha256(new Uint8Array(ikm), salt, [label, t]);
    const ref = new Uint8Array(
      hkdfSync("sha256", ikm, salt, concatBytes(utf8(label), utf8(t)), 32),
    );
    assert.deepEqual(Array.from(mine), Array.from(ref), `${label} 派生结果不一致`);
  }
});

test("AES-256-GCM 往返，且 AAD 不匹配时解密失败", async () => {
  const key = new Uint8Array(nodeRandom(32));
  const aad = utf8("v1|sid|7");
  const plain = utf8('{"t":"ping","at":1}');

  const { nonce, sealed } = await aesGcmSeal(key, aad, plain);
  assert.equal(nonce.length, 12);
  assert.equal(sealed.length, plain.length + 16, "密文尾部必须带 16 字节 tag");

  const opened = await aesGcmOpen(key, aad, nonce, sealed);
  assert.deepEqual(Array.from(opened), Array.from(plain));

  await assert.rejects(() => aesGcmOpen(key, utf8("v1|sid|8"), nonce, sealed));
});

test("信封往返：AAD 绑定 sid 与 seq", async () => {
  const key = new Uint8Array(nodeRandom(32));
  const msg = { t: "event", topic: "adapter.state", data: { phase: "host-ok" } };

  const frame = await sealMessage(key, "sid-1", 0, msg);
  assert.equal(frame.v, 1);
  assert.equal(frame.sid, "sid-1");
  assert.equal(frame.seq, 0);

  const back = await openMessage(key, "sid-1", frame);
  assert.deepEqual(back, msg);

  // 换个会话 ID 必须失败（AAD 里绑着 sid）
  await assert.rejects(() => openMessage(key, "sid-2", frame));
  // 换个序号也必须失败（AAD 里绑着 seq）
  await assert.rejects(() => openMessage(key, "sid-1", { ...frame, seq: 1 }));
});

test("transcript 拼法与核心逐字符一致", () => {
  assert.equal(
    transcript("S", "dev.a.a", "NS", "NP", "plugin"),
    "mclink-plugin-auth-v1|S|dev.a.a|NS|NP|plugin",
  );
  // 三个角色不能混用
  const base = transcript("S", "dev.a.a", "NS", "NP", "core");
  assert.notEqual(base, transcript("S", "dev.a.a", "NS", "NP", "plugin"));
  assert.notEqual(base, transcript("S", "dev.a.a", "NS", "NP", "session"));
});
