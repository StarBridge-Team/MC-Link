// MC Link 插件 SDK（Node / Bun）。
//
// 这个文件只做三件事：建连、注册处理器、收发消息的对外门面。协议细节拆在
// `handshake.js`（握手与密钥派生）、`crypto.js`（原语与信封）、`pump.js`（分发）。
//
// 零依赖：WebSocket 用运行时自带的全局 `WebSocket`（Node 22+ / Bun 都提供），
// 密码学用 WebCrypto。因此在 Node 与 Bun 上行为一致，不需要 `node_modules`。
//
// 协议细节见 `docs/plugin-developer-guide.md`。

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { event, request, PEER_CALL, PEER_EVENT, PROTOCOL_VERSION, WS_SUBPROTOCOL } from "./protocol.js";
import { createPreQueue, performHandshake, waitOpen } from "./handshake.js";
import { onText } from "./pump.js";
import { sealMessage } from "./crypto.js";

/** peer 调用的本地兜底超时（核心侧默认 10 秒）。 */
const PEER_TIMEOUT_MS = 10_000;

/** 带错误码的插件侧异常。核心会把 `code` 原样放进 `response.error.code`。 */
export class PluginError extends Error {
  /**
   * @param {string} code 错误码，见 `ErrorCode`
   * @param {string} message 面向人的说明
   */
  constructor(code, message) {
    super(message);
    this.name = "PluginError";
    this.code = code;
  }
}

/**
 * 一枚插件。
 *
 * 典型用法：
 *
 * ```js
 * const plugin = await Plugin.connectFromArgs();
 * plugin.handle("adapter.init", async () => ({ ready: true, host_fields: [] }));
 * await plugin.run();
 * ```
 */
export class Plugin {
  /**
   * 按 `--mclink-session <file>` 建立连接。
   * @param {string[]} [argv=process.argv]
   */
  static async connectFromArgs(argv = process.argv) {
    const idx = argv.indexOf("--mclink-session");
    let path = idx >= 0 ? argv[idx + 1] : undefined;
    if (!path) {
      // 兜底：核心默认追加 `--mclink-session`，但清单可以自定义参数顺序
      path = [...argv].reverse().find((a) => a && a.endsWith(".session.json"));
    }
    if (!path) {
      throw new Error("缺少 --mclink-session 参数（应由 MC Link 核心拉起插件）");
    }
    return Plugin.connect({ sessionPath: path });
  }

  /**
   * 直接按会话文件建立连接。
   * @param {{ sessionPath: string, WebSocketImpl?: typeof WebSocket }} options
   */
  static async connect({ sessionPath, WebSocketImpl = globalThis.WebSocket }) {
    if (typeof WebSocketImpl !== "function") {
      throw new Error("当前运行时不提供全局 WebSocket（需要 Node 22+ 或 Bun）");
    }
    const session = JSON.parse(readFileSync(sessionPath, "utf8"));
    for (const key of ["protocol", "plugin_id", "endpoint", "token", "psk"]) {
      if (!session[key]) throw new Error(`会话文件缺少字段: ${key}`);
    }
    if (session.protocol !== PROTOCOL_VERSION) {
      throw new Error(`协议版本不匹配：核心 ${session.protocol}，SDK ${PROTOCOL_VERSION}`);
    }

    const plugin = new Plugin(session, WebSocketImpl);
    await plugin._handshake();
    return plugin;
  }

  /** @private */
  constructor(session, WebSocketImpl) {
    this.pluginId = session.plugin_id;
    /**
     * 插件可写的数据目录（核心在拉起时创建）。
     *
     * **不要**把运行期状态写进插件安装目录：那个目录被整包验签覆盖，
     * 多一个文件就会让核心下次扫描时忽略整个插件。
     */
    this.dataDir = session.data_dir ?? null;
    this._session = session;
    this._Ws = WebSocketImpl;
    this._ws = null;
    this._pre = createPreQueue();

    this._handlers = new Map();
    this._peerHandlers = new Map();
    this._contextHandlers = [];
    this._eventHandlers = new Map();
    this._peerEventHandlers = [];
    this._shutdownHandlers = [];

    this._context = { role: "idle", game: null, room: null, peers: [] };

    this._ready = false;
    this._closed = false;
    this._closeError = null;
    this._closedResolve = null;
    this._violations = 0;
    this._lastSeq = -1;

    this._seqOut = 0;
    this._sendChain = Promise.resolve();
    this._pending = new Map();
    this._rpcSeq = 0;
  }

  // ---------------------------------------------------------------- 注册

  /**
   * 注册一个能力方法（`adapter.init` / `detector.scan` …）。
   * @param {string} method 方法名
   * @param {(params: any, ctx: any) => any | Promise<any>} handler
   */
  handle(method, handler) {
    this._handlers.set(method, handler);
    return this;
  }

  /**
   * 注册一个插件间方法：对端调用 `core.peer.call { method }` 时命中。
   * @param {string} method 私有方法名（不得使用 `adapter.` 等保留前缀）
   * @param {(params: any, ctx: any) => any | Promise<any>} handler
   */
  onPeer(method, handler) {
    this._peerHandlers.set(method, handler);
    return this;
  }

  /** 订阅 `core.context.update`（角色 / 游戏 / 房间 / 其它插件）。 */
  onContext(handler) {
    this._contextHandlers.push(handler);
    return this;
  }

  /** 按主题订阅核心推来的事件（`core.context.update` 除外，它有专门的入口）。 */
  onEvent(topic, handler) {
    const list = this._eventHandlers.get(topic) ?? [];
    list.push(handler);
    this._eventHandlers.set(topic, list);
    return this;
  }

  /** 订阅 `peer.event`（别的插件经 `core.peer.publish` 发来的广播）。 */
  onPeerEvent(handler) {
    this._peerEventHandlers.push(handler);
    return this;
  }

  /** 订阅断开（`bye` 或连接错误）。 */
  onShutdown(handler) {
    this._shutdownHandlers.push(handler);
    return this;
  }

  // ---------------------------------------------------------------- 状态

  /** 最近一次 `core.context.update` 的快照。 */
  get context() {
    return this._context;
  }

  /** 是否已断开。 */
  get closed() {
    return this._closed;
  }

  // ---------------------------------------------------------------- 主动发送

  /** 发一个单向事件（核心转发到界面）。 */
  emit(topic, data) {
    return this._sendMessage(event(topic, data));
  }

  /** 结构化日志（`plugin.log` 主题，自动带上插件 ID）。 */
  log(level, message, extra = {}) {
    return this.emit("plugin.log", { level, message, plugin: this.pluginId, ...extra });
  }

  /** 列出可调用的同机插件。 */
  async peerList() {
    return this._callCore("core.peer.list", {});
  }

  /**
   * 调用另一枚插件。
   * @param {string} to 目标插件 ID
   * @param {string} method 目标插件注册的 peer 方法名
   * @param {any} [params]
   * @param {{ depth?: number, launch?: boolean, timeoutMs?: number }} [options]
   */
  async peerCall(to, method, params = null, options = {}) {
    return this._callCore(
      "core.peer.call",
      {
        to,
        method,
        params,
        depth: options.depth ?? 0,
        launch: options.launch === true,
        timeout_ms: options.timeoutMs ?? PEER_TIMEOUT_MS,
      },
      options.timeoutMs ?? PEER_TIMEOUT_MS,
    );
  }

  /**
   * 广播（或定向投递）一条事件，不等待回执。
   * @param {string} topic
   * @param {any} data
   * @param {string} [to] 省略即广播
   */
  async peerPublish(topic, data, to) {
    return this._callCore("core.peer.publish", { topic, data, ...(to ? { to } : {}) });
  }

  // ---------------------------------------------------------------- 主循环

  /**
   * 开始处理入站消息，直到连接断开。
   *
   * 处理器由内部的帧泵驱动：`run()` 本身只是在等断开，因此可以在它之前
   * 任意注册处理器。
   */
  async run() {
    if (this._closed) {
      if (this._closeError) throw this._closeError;
      return;
    }
    this._drainPre();
    await new Promise((resolve) => {
      this._closedResolve = resolve;
    });
    if (this._closeError) throw this._closeError;
  }

  /** 主动断开。 */
  close(reason = "插件主动退出") {
    if (this._closed) return;
    // 先把 bye 放进发送链，再收尾：否则 socket 先关，对端只会看到"连接断开"
    // 而不是我们给出的原因。
    this._sendMessage({ t: "bye", reason }).finally(() => this._finish(null, reason));
  }

  // ---------------------------------------------------------------- 握手

  /** @private */
  async _handshake() {
    const url = `${this._session.endpoint}?plugin=${encodeURIComponent(
      this._session.plugin_id,
    )}&token=${encodeURIComponent(this._session.token)}`;

    const ws = new this._Ws(url, [WS_SUBPROTOCOL]);
    this._ws = ws;
    // 用 `onmessage`/`onopen` 而不是 `addEventListener`：Bun 与 Node 的 WebSocket
    // 都实现了这几个属性，但事件监听器的兼容面更窄（Bun 早期版本只有属性写法）。
    ws.onmessage = (ev) => onText(this, ev.data);
    ws.onclose = () => this._finish(this._error("unavailable", "连接被核心关闭"));
    ws.onerror = () => this._finish(this._error("unavailable", "WebSocket 连接错误"));

    await waitOpen(ws);

    try {
      const { sid, keys, heartbeatMs } = await performHandshake(
        ws,
        this._session,
        this._pre,
        pluginVersion(),
      );
      this._sid = sid;
      this._c2s = keys.c2s;
      this._s2c = keys.s2c;
      this._heartbeatMs = heartbeatMs;
    } catch (e) {
      // 握手失败时把连接收掉，别让半死的 socket 留在那里等超时
      try {
        ws.close();
      } catch {
        /* 已经关了 */
      }
      throw new PluginError(e?.code ?? "unauthorized", e?.message ?? String(e));
    }

    this._ready = true;
    this._drainPre();
  }

  // ---------------------------------------------------------------- 发送与 RPC

  /** @private 加密发送；序号与发送顺序都由这条链保证。 */
  _sendMessage(msg) {
    if (this._closed) {
      return Promise.reject(this._error("unavailable", "插件已断开"));
    }
    this._sendChain = this._sendChain
      .then(async () => {
        const seq = this._seqOut++;
        const frame = await sealMessage(this._c2s, this._sid, seq, msg);
        this._ws.send(JSON.stringify(frame));
      })
      .catch((e) => this._fail(e));
    return this._sendChain;
  }

  /**
   * @private
   * 发一个 `core.*` 请求并等待响应。
   */
  _callCore(method, params, timeoutMs = 30_000) {
    if (this._closed) {
      return Promise.reject(this._error("unavailable", "插件已断开"));
    }
    const id = `p${this._rpcSeq++}`;
    const promise = new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this._pending.delete(id);
        reject(this._error("timeout", `等待核心响应超时: ${method}`));
      }, timeoutMs + 500);
      this._pending.set(id, {
        resolve: (v) => {
          clearTimeout(timer);
          resolve(v);
        },
        reject: (e) => {
          clearTimeout(timer);
          reject(e);
        },
      });
    });
    this._sendMessage(request(id, method, params)).catch(() => {});
    return promise;
  }

  // ---------------------------------------------------------------- 收尾

  /** @private 握手完成后，把提前到达的帧补交给正常路径。 */
  _drainPre() {
    for (const raw of this._pre.drain()) onText(this, raw);
  }

  /** @private */
  _error(code, message) {
    return new PluginError(code, message);
  }

  /** @private */
  _fail(error) {
    this._finish(
      error instanceof PluginError ? error : this._error("internal", String(error?.message ?? error)),
    );
  }

  /** @private */
  _finish(error, reason) {
    if (this._closed) return;
    this._closed = true;
    this._closeError = error ?? null;
    for (const { reject } of this._pending.values()) {
      reject(error ?? this._error("unavailable", `核心断开: ${reason ?? "未知原因"}`));
    }
    this._pending.clear();
    try {
      this._ws?.close();
    } catch {
      /* 已经关了 */
    }
    for (const fn of this._shutdownHandlers) {
      try {
        fn(reason ?? (error ? error.message : "未知原因"));
      } catch {
        /* 收尾阶段的异常不该再抛出 */
      }
    }
    this._closedResolve?.();
  }
}

/**
 * @private
 * 清单版本号：核心只把它写进日志。工作目录就是插件目录（核心 `current_dir`
 * 设成 `runtime.workdir`），因此从 `plugin.json` 读即可；读不到就退回 `0.0.0`。
 */
function pluginVersion() {
  try {
    const manifest = JSON.parse(readFileSync(join(process.cwd(), "plugin.json"), "utf8"));
    return typeof manifest.version === "string" ? manifest.version : "0.0.0";
  } catch {
    return "0.0.0";
  }
}

export { CoreMethod, ErrorCode, EventTopic, PEER_CALL, PEER_EVENT, PROTOCOL_VERSION } from "./protocol.js";
export { toHex, fromHex, toBase64, fromBase64 } from "./bytes.js";
