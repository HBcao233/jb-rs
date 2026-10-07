use serde::Deserialize;

pub(super) const DETAIL_HOST: &str = "https://www.pixiv.net/touch/ajax/illust/details";

#[derive(Debug, thiserror::Error)]
pub enum PixivError {
    #[error("IO 错误: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("请求失败")]
    Http(#[from] wreq::Error),

    #[error("状态码错误: {0}")]
    Status(u16),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Pixiv 不存在")]
    NotFound,

    #[error("API 错误: {0}")]
    Api(String),
}

#[derive(Deserialize)]
pub(super) struct PixivResult {
    pub(super) body: serde_json::Value,
    pub(super) error: bool,
    pub(super) message: String,
}

#[derive(Debug, Deserialize)]
pub struct PixivDetails {
    pub author_details: AuthorDetails,
    pub illust_details: IllustDetails,
}

#[derive(Debug, Deserialize)]
pub struct AuthorDetails {
    pub user_account: String,
    pub user_id: String,
    pub user_name: String,
}

#[derive(Debug, Deserialize)]
pub struct IllustDetails {
    // mask_reason: "login_only"
    pub mask_reason: Option<String>,
    pub id: String,
    pub r#type: String,
    // ai_type == 1: 非AI, 2: AI生成
    pub ai_type: i32,
    pub page_count: String,
    pub display_tags: Vec<Tag>,
    pub tags: Vec<String>,
    pub width: String,
    pub height: String,
    pub manga_a: Option<Vec<Manga>>,
    pub url: Option<String>,
    pub url_big: Option<String>,
    pub url_placeholder: Option<String>,
    pub url_s: Option<String>,
    pub url_ss: Option<String>,
    pub user_id: String,
    pub author_details: AuthorDetails,
    pub title: String,
    pub comment: Option<String>,
    pub comment_html: Option<String>,
    pub upload_timestamp: i64,
    pub ugoira_meta: Option<LessUgoiraMeta>,
}

#[derive(Debug, Deserialize)]
pub struct Tag {
    pub tag: String,
    pub translation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Manga {
    pub page: u32,
    // pub url: String,
    pub url_big: String,
    // pub url_small: String,
}

#[derive(Debug, Deserialize)]
pub struct LessUgoiraMeta {
    pub mime_type: String,
    pub src: String,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Deserialize)]
pub struct Frame {
    pub delay: u32,
    pub file: String,
}

#[derive(Debug, Deserialize)]
pub struct UgoiraMeta {
    pub mime_type: String,
    pub src: String,
    #[serde(rename = "originalSrc")]
    pub original_src: String,
    pub frames: Vec<Frame>,
}
