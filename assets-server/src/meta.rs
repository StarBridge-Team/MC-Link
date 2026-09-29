//! 设置项元配置数据结构
//!
//! 元配置文件位于资产服务器 `SettingMeta/<section>.yml`，
//! 用于驱动前端设置页的渲染：每个字段如何展示、有何说明、可选项等。
//! 前端通过 `get_setting_meta(section)` 拉取，配合当前 `get_setting(section)`
//! 返回的 YAML 内容渲染出可交互的表单。

use serde::{Deserialize, Serialize};

/// 一个设置分区的元配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingMeta {
    /// 分区标识（与文件名一致，如 "network"、"adapter"）
    pub section: String,
    /// 分区显示标题（如 "网络设置"）
    pub title: String,
    /// 分区图标（Bootstrap Icons 类名，如 "bi-wifi"）
    #[serde(default)]
    pub icon: String,
    /// 分区描述
    #[serde(default)]
    pub description: String,
    /// 字段列表
    pub fields: Vec<FieldMeta>,
}

/// 单个字段的元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMeta {
    /// 字段 key（对应 YAML 中的键名）
    pub key: String,
    /// 显示标题
    pub label: String,
    /// 字段描述/解释
    #[serde(default)]
    pub description: String,
    /// 字段类型
    #[serde(default = "default_field_type")]
    #[serde(rename = "type")]
    pub field_type: FieldType,
    /// 默认值（字符串形式，便于直接写入 YAML）
    #[serde(default)]
    pub default: String,
    /// 可选值（用于 select / chips 类型）
    #[serde(default)]
    pub options: Vec<FieldOption>,
    /// 单位（如 "Mbps"、"ms"）
    #[serde(default)]
    pub unit: String,
    /// 数字最小值
    #[serde(default)]
    pub min: Option<f64>,
    /// 数字最大值
    #[serde(default)]
    pub max: Option<f64>,
    /// 数字步长
    #[serde(default)]
    pub step: Option<f64>,
    /// 占位符文本
    #[serde(default)]
    pub placeholder: String,
    /// 是否敏感（如密码，前端用 password 输入框）
    #[serde(default)]
    pub sensitive: bool,
    /// 是否启用自动保存（input 即时保存）
    #[serde(default = "default_true")]
    pub auto_save: bool,
    /// 分组标题（用于将字段分组展示）
    #[serde(default)]
    pub group: String,
}

fn default_field_type() -> FieldType { FieldType::Text }
fn default_true() -> bool { true }

/// 字段类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// 单行文本
    Text,
    /// 多行文本
    Textarea,
    /// 数字
    Number,
    /// 开关
    Switch,
    /// 下拉选择
    Select,
    /// 标签 chips
    Chips,
    /// 滑块
    Slider,
    /// 密码
    Password,
}

/// 字段可选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldOption {
    /// 选项值
    pub value: String,
    /// 选项显示文本
    pub label: String,
    /// 选项描述
    #[serde(default)]
    pub description: String,
}

impl SettingMeta {
    /// 从 YAML 文本解析
    pub fn parse(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|e| format!("解析元配置失败: {}", e))
    }
}
