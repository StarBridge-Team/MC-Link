pub(crate) mod adapter;
pub(crate) mod app;
pub(crate) mod clipboard;
pub(crate) mod community;
pub(crate) mod legal;
pub(crate) mod setup;
pub(crate) mod window;

pub(crate) use adapter::*;
pub(crate) use app::*;
pub(crate) use clipboard::*;
pub(crate) use community::*;
pub(crate) use legal::*;
pub(crate) use setup::*;
pub(crate) use window::*;

pub(crate) use crate::assets::{get_asset_url, read_asset_text};
pub(crate) use crate::config::push::*;
pub(crate) use crate::plugin::commands::*;
pub(crate) use crate::plugin::connect::*;
pub(crate) use crate::setting_meta::{
    clear_setting_meta_cache_command, get_setting_manifest, get_setting_meta,
};
pub(crate) use crate::update::*;
