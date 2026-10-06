use serde::Deserialize;

pub(super) const PICGO_HOST: &str = "https://www.picgo.net/api/1/upload";
pub(super) const POSTIMAGE_HOST: &str = "https://postimages.org/json/rr";

#[derive(Debug, thiserror::Error)]
pub enum UploadError {
    #[error("IO 错误: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("请求失败")]
    Http(#[from] wreq::Error),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("API 错误: {0}")]
    Api(String),
}

#[derive(Deserialize)]
pub(super) struct PicgoResult {
    pub(super) status_code: i32,
    pub(super) error: Option<ErrorMessage>,
    pub(super) image: Option<PicgoImage>,
}

#[derive(Deserialize)]
pub(super) struct ErrorMessage {
    pub(super) message: String,
}

#[derive(Deserialize)]
pub(super) struct PicgoImage {
    pub(super) url: String,
}

#[derive(Deserialize)]
pub(super) struct PostimageResult {
    pub(super) error: Option<ErrorMessage>,
    pub(super) url: Option<String>,
}
