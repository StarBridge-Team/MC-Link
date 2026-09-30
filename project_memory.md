# 项目记忆 - MC Link

> 此文件记录项目的重要架构决策和用户明确要求，供后续开发和维护参考。

## 联机核心（2026-09-30 更新）

### 现状：与 WGP Core 的连接已暂时断开

- 已移除 `wgp-core` 的跨目录 path 依赖及其 Tauri 命令。**项目现在可独立构建，不再需要仓库外的 `mc-link-core`。**
- 过渡期客户端**不含可用的联机功能**：联机页（`components/connect/`）与 `lib/api/connect.ts` 保留，但调用会失败。
- **用户明确要求：上述前端文件保持现状——不要修改、不要删除、不要"修复"。**
  它们将作为 UI 重构与插件系统的素材；联机能力在那一阶段之后再重新接入。
  过渡期点击"开始联机"报 `command not found` 属于**预期现象**，不是 bug，无需处理。
- **重新接入参考**：`legacy-wgp-p2p-v0.4.0` tag，包含：
  - `src-tauri/src/commands/connect.rs`（3 个命令 + `p2p-event` 事件）
  - `src-tauri/src/mgr/connection.rs`（ConnectionManager 状态机）
  - `src-tauri/Cargo.toml` 中的 `wgp-core` 依赖行
- **重接要点**：`wgp-core` 必须放在本仓库上一级的 `Rust/mc-link-core/`；命令为
  `start_p2p_connection` / `stop_p2p_connection` / `get_p2p_status`；事件名 `p2p-event`；
  还需恢复 `mgr/mod.rs`、`commands/mod.rs`、`lib.rs` 中的模块声明、`app.manage(...)` 与命令注册。

### 另一条独立链路：陶瓦适配器（已并入插件系统）

- 陶瓦联机封装**与 WGP 无关**，不要因为"联机断开"而删除。
- 2026-09-30 起已规范化为**内置适配器插件**：`builtin/terracotta.rs` 是唯一入口，
  `adapter.rs` 退居为该插件的运行载体，`terracotta_client.rs` 已删除。
  `commands/adapter.rs` 的 6 个命令签名不变，内部改为经插件管理器派发。
- 设计说明见仓库根目录 `插件系统设计.md`。

### 进行中的方向

- **插件系统后端已落地**（`src-tauri/src/plugin/`）：三类能力（适配/检测/耦合）、
  回环 WebSocket 控制面、权限与加密。检测类与耦合类目前只有接口与路由，尚无内置实现。
- 下一步：前端 UI 大幅重构，含插件管理界面；联机能力在新流程下经插件系统接入。

### 适配器安装的完整性校验（2026-09-30）

- 适配器包是**第三方可执行文件**，安装前必须通过 SHA256 校验。校验清单托管在我们自己的资源服务器：
  `<assets_server>/adapter/manifest.json`。客户端 **fail-closed**——拉不到清单即拒绝安装，
  不会退回无校验下载。
- 清单由 `scripts/sync-assets.mjs` 的 `prepareAdapters()` 生成（下载上游发布包 → 计算 SHA256 →
  写入 `assets-server/Assets/adapter/`），随 `pnpm sync:assets` 上传；资源服务器新增了 `/adapter/*` 路由。
- `adapter/` 目录**不参与** `Assets/manifest.json` 的版本聚合，否则每次适配器升级都会让客户端重下全部字体/图标。
- 升级适配器：改 `sync-assets.mjs` 顶部的 `ADAPTER_VERSION` / `ADAPTER_URL`，再执行 `pnpm sync:assets`。
- 若将来适配器改为随应用内嵌（`tauri.conf.json` 的 `bundle.resources`），应在
  `src-tauri/src/assets/adapter.rs` 的 `fetch_manifest` 中增加"本地常量清单"分支。

## UI 重构期间的约定（2026-09-30 起，进行中）

**当前阶段**：用户正在做前端大面积 UI 重构；这轮**前端不接后端逻辑**，UI 完成后由 AI 负责把
后端能力接上去（含插件系统接线与联机能力恢复）。

### 重构期间保持"跨层契约"不变（改了我需要同步改后端，改动前先告知）

| 契约 | 位置 | 为什么 |
|---|---|---|
| `PersonalizationSettings` 字段名 | 后端 `config/mod.rs` ↔ 前端 `lib/api/types.ts` | 字段名即 yml 的 key，改名等于让老用户配置失效 |
| `lib/persist.ts` 的 `KEYS` 字符串 | `src/lib/persist.ts` | 改名会让老用户 localStorage 变孤儿（表现为设置丢失） |
| 窗口效果取值集合 | `mica` / `acrylic` / `hud_window` / `none` | 后端 `effect::apply_effect_by_name` 按名称匹配 |
| 事件名与负载结构 | `app-log`、`download-progress`、`update-progress`、`p2p-event`、`tray-resize`、`deep-link` | 后端 emit 侧未变，前端改名会监听不到 |
| 更新清单格式与命令名 | `update/latest.json` ↔ `src/lib/api/update.ts` | 见下文"应用更新"章节 |

### 重构期间必须继续遵守的两条硬规则（与本次重构无关，属长期规范）

- `invoke(...)` 只允许出现在 `src/lib/api/**`，页面与组件必须经 api 层。
- 本地存储只经 `src/lib/persist.ts`，不得直接写 `localStorage` / `sessionStorage`。

### 不要删除的东西（用户已明确要求保留）

- 联机页 4 个组件（`components/connect/`）+ `lib/api/connect.ts`：作为 UI 重构素材保留，
  调用会失败属预期（见上文"联机核心"章节）。

### UI 完成后需要 AI 接线的后端能力（后端已实现，前端目前零调用）

| 能力 | 后端状态 | 前端缺口 |
|---|---|---|
| 插件系统 | `src-tauri/src/plugin/**` 完整（8 个命令 + 回环网关 + 权限/加密） | 无 `lib/api/plugin.ts`，无插件管理界面 |
| 联机（P2P） | 已按用户要求**暂时断开** | 联机页保留但调用失败；重接见 `legacy-wgp-p2p-v0.4.0` |
| 检查更新 | ✅ 已完整实现（两种安装形态，见下节） | api + composable 已就绪，**只缺界面** |
| 玩家名 | `Setting/account.yml` 的 `player_name` | 只读不写，无写入入口 |
| 分区设置 | `save_setting` 命令存在 | `connector.yml` / `account.yml` 无设置界面 |

## 应用更新（2026-09-30 实现）

### 前置闸门：构建渠道（先看这个，再看下面）

自动更新**替换的是应用自身**，所以先得判断"这份二进制是什么来路"。
判定在 `src-tauri/src/build_channel.rs`，输入由 `build.rs` 从构建环境转发：

| 渠道 | 何时出现 | 能否自动更新 |
|---|---|---|
| `official` | `pnpm build:release` / `pnpm tauri build`（发布流程注入 `MC_LINK_BUILD_CHANNEL=official`） | ✅ 可以 |
| `dev` | debug 编译（含 `pnpm tauri dev`） | ❌ **连检查都不发起**（不打无意义的网络请求）；调试更新流程时用 `MC_LINK_UPDATE_OVERRIDE=1` 强制开启 |
| `self-built` | release 编译但没走发布流程（`cargo build --release`、`--no-release`、fork 自编译） | ❌ 只提示"官方有新版本" + 手动下载链接，**绝不替换文件** |

两条关键规则：

- **debug 优先于标记**：带 official 标记的 debug 构建仍视为开发构建，否则 `cargo build`
  时残留的环境变量就能把调试环境变成自更新目标。
- **自构建版本不认 `MC_LINK_UPDATE_OVERRIDE`**：那个开关只给开发构建用，
  因为自构建的 release 可能已经分发给别人，不能让一个环境变量绕过限制。

闸门有两处：`update/fetch.rs`（决定是否检查、是否给可安装资产）与 `update/install.rs`
（落地前二次确认，防绕过界面直接调用命令）。已实测四种组合（release±标记、debug±override）。

**CI 注意**：CI 若直接调用 `pnpm exec tauri build`（而非 `scripts/tauri-build.mjs`），
产物会被视为 `self-built` 而无法自动更新。

### 两种安装形态（这是本次更新的核心）

| 形态 | 判定 | 更新方式 |
|---|---|---|
| **便携版** | exe 同目录存在 `data/`、`portable.txt` 或 `portable` | 下载单个 exe，**直接替换自身**，随后重启 |
| **安装版** | 其余情况（数据在系统 app_data_dir） | 下载 NSIS 安装器并运行，装完由我们重新拉起应用 |

判定逻辑在 `src-tauri/src/datadir.rs::install_mode()`（与数据目录判定共用同一套约定）。
**不要把安装版包塞给便携版**：`install.rs` 会拒绝"形态与包类型不匹配"的组合，这是刻意的。

### 落地为什么必须由外部进程做

覆盖正在运行的 exe、或运行安装器替换本目录文件，都必须发生在**本进程退出之后**。
实现方式：生成一段 PowerShell，以 `-EncodedCommand`（UTF-16LE + Base64）交给隐藏窗口子进程，
它按 PID 等待本进程退出 → 执行替换/安装 → 重新拉起应用。用 `-EncodedCommand` 而非临时脚本文件，
是为了绕开"路径含中文时的代码页"问题。日志写在 `<data_dir>/Cache/Updates/install.log`。

### 更新清单（`<assets_server>/update/latest.json`）

多资产格式，**同一份清单同时服务两种发行方式**，客户端按 `platform` + `kind` 自选：

| 字段 | 说明 |
|---|---|
| `manifest_version` | 客户端只接受 ≤ 自身支持的版本（当前 1），更高即拒绝并提示手动下载 |
| `assets[].platform` | 形如 `windows-x86_64`，与 `assets::adapter::current_platform()` 一致 |
| `assets[].kind` | `installer` / `portable` / `portable-zip`（仅手动下载） |
| `assets[].sha256` | **必填**：更新包会被直接执行，缺失即拒绝自动安装 |
| `assets[].urls` | 留空时客户端按自身资源服务器地址推导 `update/<file>`，因此清单里不必写死域名 |

### 发布流程

```
pnpm build:release            # 自增版本 + tauri build + rename-build + make-update + sync-assets
pnpm tauri build --mandatory  # 同上，并把清单标记为强制更新
node scripts/make-update.mjs --platform windows-aarch64   # 其他架构，增量合并进同一份清单
```

- 清单与更新包由 `scripts/make-update.mjs` 生成到 `assets-server/Assets/update/`（不进 git），
  更新说明取仓库根目录可选的 `release-notes.md`。
- 资源服务器 `/update/*` 从 `Assets/update/` 读取（与 `POST /upload` 布局一致），并回退旧的 `Updates/` 目录。
- `update/` 与 `adapter/` 一样**不参与** `Assets/manifest.json` 的版本聚合，
  否则每次发版都会让所有客户端重下字体与图标。

### 前端契约

- `src/lib/api/update.ts`：`checkUpdate` / `downloadUpdate` / `installUpdate` /
  `clearUpdateCache` / `getRuntimeInfo`（原 `getInstallMode` 已改名，命令为 `get_runtime_info_command`）
- `src/composables/useUpdater.ts`：`check()` → `download()`（可选，带进度）→ `install()`；
  判断依据是 `updateAllowed`（渠道是否允许）与 `canAutoInstall`（渠道 + 是否有对应包），
  不允许时显示 `blockedMessage`，并引导用户走 `manualUrl`
- 进度事件 `update-progress`，负载 `{ downloaded, total }`（`total` 为 0 表示长度未知）
- `install()` 成功后应用会在约 0.6 秒内退出：**先给用户提示再调用**
- `check()` 返回的 `build_channel` / `update_allowed` 与 `RuntimeInfo` 里的字段同义，
  界面应据此区分"已是最新版本"与"当前构建不参与自动更新"这两种情况

### 两条落地路径（2026-09-30 起）

| 场景 | 谁落地 | 读哪份清单 |
|---|---|---|
| Windows **便携版** | **自研**（`update/install.rs`：替换 exe + 外部进程重启） | `latest.json` |
| Windows **安装版** | **官方插件**（`tauri-plugin-updater`，NSIS passive），失败回退自研 | `tauri.json` |
| **Linux** | **官方插件**（仅 AppImage 支持自动安装；deb/rpm 不支持） | `tauri.json` |
| **macOS** | **官方插件**（`.app.tar.gz`） | `tauri.json` |

官方插件只能处理 NSIS/MSI/AppImage/`.app.tar.gz`，对"exe 同目录即全部"的便携形态无能为力，
因此便携版必须留在自研路径。反过来，插件需要**签名公钥**才能用。

**`pubkey` 就是插件路径的开关**：`tauri.conf.json` 的 `plugins.updater.pubkey` 为空（或仍是
`REPLACE_ME` 占位符）时不注册插件、整条插件路径不可用，安装版自动回退到自研的安装器拉起。
因此**没配密钥也不会出现"更新不了"**。

### 启用官方插件需要做三件事（目前尚未配置）

```powershell
# 1. 生成密钥对（私钥务必妥善保管：丢了以后已安装的用户再也收不到更新）
#    私钥必须放在仓库之外。项目内曾有 .tauri/updater.key，已从索引移除并加入 .gitignore。
pnpm exec tauri signer generate -w $env:USERPROFILE\.tauri\mclink.key
# 2. 把输出的公钥内容（不是路径）填进 src-tauri/tauri.conf.json 的 plugins.updater.pubkey
# 3. 发布时提供私钥，脚本会自动签名并生成 tauri.json
$env:TAURI_SIGNING_PRIVATE_KEY_PATH = "$env:USERPROFILE\.tauri\mclink.key"
pnpm build:release
```

> **安全：历史提交里的私钥必须轮换。**
> `.tauri/updater.key`（加密的 minisign 私钥）与 `.pub` 在提交 `9ee632d` 中被跟踪，
> 此后一直在版本历史里。把它从索引移除**不会**从历史中删除，因此这份密钥要视为已泄露。
> 轮换成本为零：此前从未发布过使用该密钥的更新（插件路径是 2026-09-30 才接入的），
> 没有任何已安装版本依赖它。生成新密钥后把新公钥填进 `plugins.updater.pubkey` 即可。
>
> 另注：`分析.md` 把 `.tauri/` 列为"Tauri 更新密钥"目录，正是这种做法的源头；
> 该文件已列为待重写，重写时不要再保留这条。

注意 Tauri CLI 区分两个变量：`TAURI_SIGNING_PRIVATE_KEY_PATH` 收**路径**、
`TAURI_SIGNING_PRIVATE_KEY` 收**内容**。`make-update.mjs` 两种写法都接受
（把路径写进 `_KEY` 会自动纠正）。

CI 里必须用**环境变量**提供私钥（`.env` 文件无效），并保持私钥在 secret 中。

### 清单与产物的生成（`scripts/make-update.mjs`）

- `latest.json`：我们的清单，**版本判定始终以它为准**（含 `mandatory`、`manual_url` 等）。
- `tauri.json`：官方插件的清单（`platforms` + minisign 签名），只负责告诉插件"下载地址 + 签名"。
  没有签名私钥时**不生成**——签名校验不可关闭，生成一份通不过校验的清单只会让人误以为配好了。
- Linux/macOS 产物（`.AppImage` / `.app.tar.gz`）只在对应平台构建时才存在，脚本会自动发现。
- 平台键差异：我们用 `macos-*`，插件要求 `darwin-*`，脚本里做转换。

### 已知取舍与限制

- **Linux 的 deb/rpm 不支持自动更新**（官方插件只认 AppImage），macOS 需要签名与 Gatekeeper。
  这两类用户只能手动下载。
- 便携版更新包用的是**未压缩的 exe**（当前约 17MB），而便携整包 zip 只有约 7MB。
  换成 zip 需要引入解压依赖且要处理多文件覆盖，当前按"简单可靠"取舍（用户已确认先不管）。
- **旧包会按版本回收**（客户端 + 服务端都做，见下节）；服务端保留最新两个版本。
- 接入插件时 `wry` 被动升级 `0.55 → 0.57`（插件的传递依赖），已通过 `cargo build` 验证。

### 更新相关缓存的回收（2026-09-30）

三层，策略**刻意不同**：

| 位置 | 策略 | 为什么 |
|---|---|---|
| 客户端 `Cache/Updates/` | 启动时删除**版本 ≤ 当前应用版本**的包与 `.tmp` 残留；认不出的文件名一律保留 | 装过的包已经用完；比当前版本新的包要留着（用户可能选了"稍后安装"） |
| 本地 `assets-server/Assets/update/` | `make-update` 只保留**本次版本**（含其他架构） | 本地是上传源，留旧包只会每次重复上传；但同版本的其他架构必须保留，否则清单会指向缺失文件 |
| 远端服务器 `Assets/update/` | `sync-assets` 上传后回收，保留**最新两个版本** | 远端是分发点，留一个旧版本作缓冲，避免正在下载旧版的客户端中断 |

服务端为此新增两个端点：`GET /update/list`（列出文件名）与
`POST /delete?path=<相对路径>&token=`（与上传同样是 token 保护 + 防路径穿越）。
在此之前服务器**只有上传没有任何删除能力**，旧版本包（每版约 30MB）会永久堆积。

- 客户端入口：`update::download::prune_cache`（由 `update::cleanup_leftovers` 在启动时调用，有单测）
- 远端入口：`scripts/sync-assets.mjs` 的 `pruneRemoteUpdates`

## 持久化约定（2026-09-30 起强制执行）

### 唯一入口

| 层 | 唯一入口 | 谁负责 |
|---|---|---|
| Rust 用户数据 | `src-tauri/src/persist.rs` | 原子写入（tmp+rename）、损坏隔离（改名为 `.corrupt-<时间戳>` 而非覆盖）、失败打印原因 |
| 前端本地存储 | `src/lib/persist.ts` | `KEYS` 集中登记 key；`local` / `session` 命名空间；失败 `console.warn` |

### 禁止事项

- **禁止**在 Rust 侧用 `std::fs::write` 直接写用户数据（Setting/*.yml、Adapter/*.json、
  Plugins/registry.json、插件密钥都必须走 `crate::persist`）。`Assets/` 是可重新下载的
  缓存，仍走 `clear_cache` 语义，不受此约束。
- **禁止**在前端直接使用 `localStorage` / `sessionStorage`，一律经 `lib/persist.ts`。
- **禁止**"解析失败就用默认值覆盖原文件"——必须隔离备份后重建，并打印日志。
  这条是"软件回到全新状态"的已知成因之一。
- **禁止**改动 `KEYS` 里的 key 字符串。改名会让老用户的 localStorage 数据变孤儿
  （表现为设置丢失）；确需改名必须先写迁移。

### 为什么

用户报告过"退出时改动没保存、重启后回到全新状态"。排查确认：个性化保存链路本身正常，
真正的风险来自 ① 多处直接 `std::fs::write` + 静默覆盖（损坏即整体清空且无法取证）、
② 前端存储封装写好却没人用、失败被静默吞掉、`player_name` 甚至只读不写。

### 已知的"未接线"项（不是 bug，重构时一并处理）

- 玩家名（`KEYS.playerName`）目前**没有写入入口**，只有读取，所以改了也不会保存。
- `save_setting` 命令（`conector.yml` / `account.yml` 等分区）前端**无任何调用者**，
  这些分区的设置界面已随 `MetaSettingSection.vue` 一起移除。

## 认证方案（2026-09-30 更新）

### 现状：客户端不含任何登录功能

- **已移除全部认证实现**，包括：
  - 客户端：登录、Token 校验、桌面登录 init/poll、用户信息与头像代理、Token 解密
  - 服务端：`central-server/` 整体移除（该服务将另行重写，不在本仓库）
- **不要重新引入** Token 登录、OAuth 登录、账号会话或任何自建认证服务。
- 历史实现保留在 `legacy-auth-central-v0.4.0` tag，仅用于查阅，不再维护。

### 后续方向

- 账号鉴权将由 **OpenXigoID** 与 **Prism Auth** 实现。
- 现阶段**暂不考虑登录功能**，客户端所有功能均在未登录状态下可用。
- 集成时以上述两者为唯一鉴权入口，不再自建认证服务与账号存储。

## 易被误删的非认证内容

- `Setting/account.yml` 的字段是 `player_name`（玩家显示名），**与认证无关**。
- `Setting/connector.yml` 是 MC 局域网扫描配置（`default_port`、`lan_scan_enabled`），**与 WGP 无关**。
- `deep_link`（`mclink://` 协议）是通用 URL 分发机制，非认证专用，保留待用。
- `uapi-sdk-rust` 用于 `get_ip_info`（网络 IP 归属查询），非认证依赖，保留。

## 历史决策（已作废，仅作溯源）

_以下内容记录于 2026-06-11，其描述的 Token / OAuth 认证架构已随认证实现一并移除，不再适用。_

- 管理面板只保留 Token 登录、移除 OAuth 登录入口。
- 客户端通过独立页面获取 token，方式为：`@qq.com` 邮箱登录，或 OAuth 单点登录
  （GitHub、Microsoft、LittleSkin、MSL 用户中心）。
- 客户端将 token 交给中央服务器，由中央服务器调用账号 API 验证 token 有效性。
- 当时状态：`oauth.rs`、`session.rs` 已实现；管理面板同时支持 Token 与 OAuth，待重构。
