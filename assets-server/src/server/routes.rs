//! 路由分发。
//!
//! 这里只做"看 URL 决定交给谁"，具体怎么响应在 [`super::files`] / [`super::upload`] 里。
//! 顺序有意义：`/update/list` 必须排在 `/update/` 前缀匹配之前，否则会被后者吃掉。

use std::path::Path;
use std::sync::Arc;

use tiny_http::{Method, Request};

use crate::config::AssetsConfig;

use super::files::{
    list_update_files, serve_file, serve_meta, serve_meta_index, serve_update, ServeCtx,
};
use super::paths::sanitize;
use super::response::{
    bad_request, cors_preflight, header_value, json_response, method_not_allowed, not_found, Resp,
};
use super::upload::{handle_delete, handle_upload};

pub(super) fn route(req: &mut Request, cfg: &Arc<AssetsConfig>) -> Resp {
    let method = req.method().clone();
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or(&url);
    let query = url.splitn(2, '?').nth(1).unwrap_or("");

    // 上传接口：仅允许 POST /upload（文件写入 assets_dir()/<path>，与 GET 布局一致）
    if path == "/upload" {
        if method != Method::Post {
            return method_not_allowed();
        }
        return handle_upload(req, cfg, query);
    }

    // 删除接口：仅允许 POST /delete（与上传同样需要 token）。
    // 存在的理由：资源服务器只有上传没有删除时，旧版本的更新包会永久堆积
    //（每个版本约 30MB），且没有任何办法回收。
    if path == "/delete" {
        if method != Method::Post {
            return method_not_allowed();
        }
        return handle_delete(req, cfg, query);
    }

    // 仅允许 GET / HEAD / OPTIONS
    if method != Method::Get && method != Method::Head && method != Method::Options {
        return method_not_allowed();
    }

    // CORS 预检
    if method == Method::Options {
        return cors_preflight(&cfg.cors_origin);
    }

    // 把 Range 与 CORS 一起传给文件响应函数：tiny_http 不处理 Range，
    // 断点续传完全由我们实现（详见 files::serve_abs）
    let ctx = ServeCtx {
        cors: &cfg.cors_origin,
        range: header_value(req, "Range"),
    };

    match path {
        "/health" => json_response(r#"{"status":"ok"}"#, &cfg.cors_origin),

        // 资源清单（客户端据此判断是否需要重新下载）
        "/manifest.json" => serve_file(&cfg.assets_dir(), "manifest.json", &ctx),

        // Bootstrap Icons
        p if p.starts_with("/bootstrap-icons/") => {
            let rel = p.trim_start_matches("/bootstrap-icons/");
            serve_file(&cfg.assets_dir().join("bootstrap-icons"), rel, &ctx)
        }

        // 字体
        p if p.starts_with("/fonts/") => {
            let rel = p.trim_start_matches("/fonts/");
            serve_file(&cfg.assets_dir().join("fonts"), rel, &ctx)
        }

        // 图标（应用图标、托盘图标等）
        p if p.starts_with("/icons/") => {
            let rel = p.trim_start_matches("/icons/");
            serve_file(&cfg.assets_dir().join("icons"), rel, &ctx)
        }

        // 适配器校验清单（客户端据此校验适配器包的 SHA256）
        "/adapter/manifest.json" => {
            serve_file(&cfg.assets_dir().join("adapter"), "manifest.json", &ctx)
        }

        // 适配器包文件（存放于 Assets/adapter/，与上传端点布局一致）
        p if p.starts_with("/adapter/") => {
            let rel = p.trim_start_matches("/adapter/");
            serve_file(&cfg.assets_dir().join("adapter"), rel, &ctx)
        }

        // 页面清单
        "/pages/manifest.json" => serve_file(&cfg.pages_dir(), "manifest.json", &ctx),

        // 单个页面内容
        p if p.starts_with("/pages/") => {
            let rel = p.trim_start_matches("/pages/");
            // 若调用方未带后缀，默认补 .html
            let rel = if Path::new(rel).extension().is_some() {
                rel.to_string()
            } else {
                format!("{}.html", rel)
            };
            serve_file(&cfg.pages_dir(), &rel, &ctx)
        }

        // 更新目录文件清单（供发布脚本回收旧版本包；必须排在下面的前缀匹配之前）
        "/update/list" | "/updates/list" => list_update_files(&cfg),

        // 更新清单
        "/update/latest.json" | "/updates/latest.json" => serve_update(&cfg, "latest.json", &ctx),

        // 更新包文件
        p if p.starts_with("/update/") || p.starts_with("/updates/") => {
            let rel = p
                .strip_prefix("/update/")
                .or_else(|| p.strip_prefix("/updates/"))
                .unwrap_or("");
            serve_update(&cfg, rel, &ctx)
        }

        // 设置项清单（列出所有可用的设置分区）
        "/settings/manifest.json" => serve_file(&cfg.setting_meta_dir(), "manifest.json", &ctx),

        // 设置项元配置
        p if p.starts_with("/settings/meta/") => {
            let section = p.trim_start_matches("/settings/meta/");
            let safe = sanitize(section);
            if safe.is_empty() {
                return bad_request("无效的 section");
            }
            let file = format!("{}.yml", safe);
            serve_meta(&cfg.setting_meta_dir(), &file, ctx.cors)
        }

        // 整个 SettingMeta 目录索引（列出可用的 section）
        "/settings/meta" | "/settings/meta/" => {
            serve_meta_index(&cfg.setting_meta_dir(), &cfg.cors_origin)
        }

        // 法务文件：EULA 与许可证全文（GPLv3、第三方声明等）。
        //
        // 放在资源服务器而不是编译进客户端，是为了**不重新发版就能更新条款文本**；
        // 客户端拉取后缓存，供首次引导与"关于"页离线展示。
        // 与 adapter 一样置于 Assets/ 之下，这样发布脚本不必改上传逻辑即可送达。
        "/legal/manifest.json" => {
            serve_file(&cfg.assets_dir().join("legal"), "manifest.json", &ctx)
        }

        p if p.starts_with("/legal/") => {
            let rel = p.trim_start_matches("/legal/");
            let safe = sanitize(rel);
            if safe.is_empty() {
                return bad_request("无效的文件名");
            }
            serve_file(&cfg.assets_dir().join("legal"), &safe, &ctx)
        }

        _ => not_found(),
    }
}
