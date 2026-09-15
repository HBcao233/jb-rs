use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct AwemeDetail {
    pub author: Author,
    pub aweme_id: String,
    pub item_title: String,
    pub desc: String,
    pub aweme_type: i32,
    pub video: Option<Video>,
    pub images: Option<Vec<Image>>,
}

#[derive(Debug, Deserialize)]
pub struct Author {
    pub sec_uid: String,
    pub nickname: String,
}

#[derive(Deserialize)]
pub(super) struct AwemeResult {
    pub(super) aweme_detail: Option<AwemeDetail>,
    pub(super) status_code: i32,
}

#[derive(Debug, Deserialize)]
pub struct Video {
    pub duration: u32,
    pub play_addr: Addr,
    pub download_addr: Option<Addr>,
    pub origin_cover: Addr,
}

#[derive(Debug, Deserialize)]
pub struct Addr {
    pub url_list: Vec<String>,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Deserialize)]
pub struct Image {
    pub url_list: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum GetAwemeError {
    #[error("HTTP 请求失败: {0}")]
    Http(#[from] wreq::Error),

    #[error("状态码错误: {0}")]
    Status(u16),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("推文不存在")]
    NotFound,

    #[error("API 错误: {0}")]
    Api(String),
}
