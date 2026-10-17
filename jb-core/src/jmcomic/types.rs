use serde::Deserialize;

/// jmcomic 请求错误
#[derive(Debug, thiserror::Error)]
pub enum JMError {
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

    #[error("解码错误")]
    Base64(#[from] base64::DecodeError),

    #[error("解密错误")]
    Aes(#[from] aes::cipher::block_padding::Error),

    #[error("字符串解析为int失败: {0}")]
    ParseInt(#[from] std::num::ParseIntError),

    /// album not found
    #[error("本子不存在")]
    NotFound,

    /// Api error
    #[error("API 错误: {0}")]
    Api(String),
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
pub(super) struct DomainResult {
    pub(super) jm3_Server: Vec<[String; 2]>,
}

#[derive(Deserialize)]
pub(super) struct JMResult {
    pub(super) code: i32,
    pub(super) data: serde_json::Value,
}

/// 本子信息
#[derive(Debug, Deserialize)]
pub struct AlbumInfo {
    /// 创建时间, 秒级时间戳
    pub addtime: String,
    /// 作者
    pub author: Vec<String>,
    /// 简介
    pub description: String,
    /// JM id
    pub id: i32,
    /// 图片列表, 一般为空
    pub images: Vec<String>,
    /// 点赞数
    pub likes: String,
    /// 本子名
    pub name: String,
    /// 价格
    pub price: String,
    /// 相关作品
    pub related_list: Vec<RelatedAlbum>,
    /// 系列
    pub series: Vec<String>,
    /// 系列id
    pub series_id: String,
    /// 标签
    pub tags: Vec<String>,
    /// 图片数量
    pub total_photos: u16,
    /// 浏览量
    pub total_views: String,
}

#[derive(Debug, Deserialize)]
pub struct RelatedAlbum {
    pub author: String,
    pub id: String,
    pub image: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct Chapter {
    /// 创建时间, 秒级时间戳
    pub addtime: String,
    /// JM id
    pub id: i32,
    /// 图片列表
    pub images: Vec<String>,
    /// 系列
    pub series: Vec<String>,
    /// 系列id
    pub series_id: String,
    /// 标签, 以空格连接
    pub tags: String,
}
