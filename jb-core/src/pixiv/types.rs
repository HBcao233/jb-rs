//! types

use serde::{Deserialize, Deserializer};

pub(super) const DETAIL_HOST: &str = "https://www.pixiv.net/touch/ajax/illust/details";

/// Pixiv 请求错误
#[derive(Debug, thiserror::Error)]
pub enum PixivError {
    /// io error
    #[error("IO 错误: {0}")]
    Io(#[from] tokio::io::Error),

    /// http error
    #[error("请求失败")]
    Http(#[from] wreq::Error),

    /// StatusCode is not 200
    #[error("状态码错误: {0}")]
    Status(u16),

    /// json error
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// pixiv not found.
    #[error("Pixiv 不存在")]
    NotFound,

    /// Api error
    #[error("API 错误: {0}")]
    Api(String),
}

#[derive(Deserialize)]
pub(super) struct PixivResult {
    pub(super) body: serde_json::Value,
    pub(super) error: bool,
    pub(super) message: String,
}

/// Pixiv Artwork 作品信息
#[derive(Debug, Deserialize)]
pub struct PixivDetails {
    /// 作者信息
    pub author_details: AuthorDetails,
    /// 插画信息
    pub illust_details: IllustDetails,
}

/// Pixiv 作者信息
#[derive(Debug, Deserialize)]
pub struct AuthorDetails {
    /// 用户名
    pub user_account: String,
    /// id
    pub user_id: String,
    /// 作者名
    pub user_name: String,
}

/// 插画信息
#[derive(Debug, Deserialize)]
pub struct IllustDetails {
    /// 无 url 原因
    /// "login_only"
    pub mask_reason: Option<String>,
    /// pixiv illust id
    pub id: String,

    /// 投稿类型
    ///
    /// - 1: 正常
    /// - 2: 动图
    pub r#type: String,

    /// 是否为ai
    ///
    /// - 1: 非AI
    /// - 2: AI生成
    pub ai_type: i32,

    /// 图片数量
    pub page_count: String,
    /// 标签详细信息
    pub display_tags: Vec<Tag>,
    /// 标签
    pub tags: Vec<String>,
    /// 宽
    pub width: String,
    /// 高
    pub height: String,
    /// page_count > 1 时, 多图投稿信息
    pub manga_a: Option<Vec<Manga>>,
    /// master 图片链接
    pub url: Option<String>,
    /// 原图链接
    pub url_big: Option<String>,
    /// 100x100 小图
    pub url_placeholder: Option<String>,
    /// small 图片链接
    pub url_s: Option<String>,
    /// small 图片链接
    pub url_ss: Option<String>,
    /// 作者id
    pub user_id: String,
    /// 作者详情
    pub author_details: AuthorDetails,
    /// 作品标题
    pub title: String,
    /// 作品详情
    pub comment: Option<String>,
    /// 作品详情 html
    pub comment_html: Option<String>,
    /// 上传时间
    pub upload_timestamp: i64,
    /// 动图信息 (缺少 originalSrc)
    pub ugoira_meta: Option<LessUgoiraMeta>,
    /// 图片分辨率信息
    pub illust_images: Vec<ImageResolution>,
}

/// 标签
#[derive(Debug, Deserialize)]
pub struct Tag {
    /// 标签名
    pub tag: String,
    /// 翻译
    pub translation: Option<String>,
}

/// 多图分 P
#[derive(Debug, Deserialize)]
pub struct Manga {
    /// 序号, 从 0 开始
    pub page: u32,
    // pub url: String,
    /// 原图链接
    pub url_big: String,
    // pub url_small: String,
}

/// 动图信息 (缺少 originalSrc)
#[derive(Debug, Deserialize)]
pub struct LessUgoiraMeta {
    /// mime type
    pub mime_type: String,
    /// 压缩包链接
    pub src: String,
    /// 帧信息
    pub frames: Vec<Frame>,
}

/// 帧信息
#[derive(Debug, Deserialize)]
pub struct Frame {
    /// 延迟
    pub delay: u32,
    /// 文件名
    pub file: String,
}

/// 动画信息
#[derive(Debug, Deserialize)]
pub struct UgoiraMeta {
    /// mime type
    pub mime_type: String,
    /// 压缩包链接
    pub src: String,
    /// 原画质压缩包链接
    #[serde(rename = "originalSrc")]
    pub original_src: String,
    /// 帧信息
    pub frames: Vec<Frame>,
}

fn deserialize_string_to_u32<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    s.parse::<u32>().map_err(serde::de::Error::custom)
}

/// 分辨率信息
#[derive(Debug, Deserialize)]
pub struct ImageResolution {
    /// 宽度
    #[serde(deserialize_with = "deserialize_string_to_u32")]
    pub illust_image_width: u32,
    /// 高度
    #[serde(deserialize_with = "deserialize_string_to_u32")]
    pub illust_image_height: u32,
}
