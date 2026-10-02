import { invoke } from "@tauri-apps/api/core";
import type { GameRecommendation, LegalBundle, SetupState } from "./types";

/**
 * OOBE 第二步（EULA）与第三步（选择首个游戏）。
 *
 * # 同意记录由后端裁决
 *
 * `accept_eula_command` 由后端**自己取正文、自己算 sha256、自己生成时间戳**，
 * 前端无法伪造"同意的是哪一版"。因此这里只传 `language`，不传版本与哈希。
 *
 * # 拉不到条款时不是错误
 *
 * 条款托管在资源服务器上；拉不到时 `legal_fetch_command` **不报错**，
 * 而是返回 `eula: null` + `fallback_url` + `error`，界面提示用户阅读官网条款并同意。
 * 此路径下的同意记录会如实标注 `version/sha256 = unfetched`。
 */

/** 拉取条款清单与正文。 */
export async function fetchLegal(language: string) {
  return invoke<LegalBundle>("legal_fetch_command", { language });
}

/** 同意 EULA（版本与哈希由后端自取自算）。 */
export async function acceptEula(language: string) {
  return invoke<SetupState>("accept_eula_command", { language });
}

/** 选择首个游戏（校验 id 合法性后写入 `setup.yml`）。 */
export async function setFirstGame(gameId: string) {
  return invoke<SetupState>("set_first_game_command", { gameId });
}

/** 按游戏 id 取适配/检测/耦合三类推荐（与插件管理界面共用路由引擎）。 */
export async function gameRecommend(gameId: string) {
  return invoke<GameRecommendation>("game_recommend_command", { gameId });
}
