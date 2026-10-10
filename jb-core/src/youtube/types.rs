//! types

use serde::{Deserialize, Serialize};

pub(super) const INFO_HOST: &str = "https://www.youtube.com/youtubei/v1/player";

pub(super) const CLIENT_NAME: &str = "1";
pub(super) const CLIENT_VERSION: &str = "2.20260708.00.00";

#[allow(non_snake_case)]
#[derive(Deserialize, Default, Debug)]
pub(super) struct Ytcfg {
    pub(super) VISITOR_DATA: String,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
pub(super) struct InfoBody {
    playbackContext: PlaybackContext,
    contentCheckOk: bool,
    racyCheckOk: bool,
    context: Context,

    #[serde(rename = "videoId")]
    video_id: String,
}

impl InfoBody {
    pub(super) fn new(video_id: String) -> Self {
        Self {
            playbackContext: PlaybackContext {
                contentPlaybackContext: ContentPlaybackContext {
                    html5Preference: String::from("HTML5_PREF_WANTS"),
                },
            },
            contentCheckOk: true,
            racyCheckOk: true,
            context: Context {
                client: Client {
                    clientName: String::from("WEB"),
                    clientVersion: String::from(CLIENT_VERSION),
                    hl: String::from("zh-CN"),
                },
                thirdParty: ThirdParty {
                    embedUrl: String::from("https://google.com"),
                },
            },
            video_id,
        }
    }
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct PlaybackContext {
    #[serde(rename = "contentPlaybackContext")]
    contentPlaybackContext: ContentPlaybackContext,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct ContentPlaybackContext {
    html5Preference: String,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct Context {
    client: Client,
    thirdParty: ThirdParty,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct Client {
    clientName: String,
    clientVersion: String,
    hl: String,
}

#[allow(non_snake_case)]
#[derive(Serialize)]
struct ThirdParty {
    embedUrl: String,
}

/// Youtube 请求错误
#[derive(Debug, thiserror::Error)]
pub enum YoutubeError {
    /// IO error
    #[error("IO 错误: {0}")]
    Io(#[from] tokio::io::Error),

    /// http error
    #[error("请求失败")]
    Http(#[from] wreq::Error),

    /// StatusCode is not 200
    #[error("状态码错误: {0}")]
    Status(u16),

    /// json
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// video not found
    #[error("Youtube 视频不存在")]
    NotFound,

    /// api error
    #[error("API 错误: {0}")]
    Api(String),
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
pub(super) struct InfoResult {
    pub(super) playabilityStatus: PlayabilityStatus,
    pub(super) videoDetails: VideoDetails,
}

#[derive(Deserialize)]
pub(super) struct PlayabilityStatus {
    pub(super) status: String,
    pub(super) reason: String,
}

/// 视频信息
#[derive(Debug, Deserialize)]
pub struct VideoDetails {
    /// 作者
    pub author: String,
    /// Youtube 频道id
    #[serde(rename = "channelId")]
    pub channel_id: String,
    /// 是否允许爬虫
    #[serde(rename = "isCrawlable")]
    pub is_crawlable: bool,
    /// 时长
    #[serde(rename = "lengthSeconds")]
    pub length_seconds: String,
    /// 描述
    #[serde(rename = "shortDescription")]
    pub short_description: String,
    /// 缩略图
    pub thumbnail: ThumbnailInfo,
    /// 视频标题
    pub title: String,

    /// Youtube video id
    #[serde(rename = "videoId")]
    pub video_id: String,
}

/// 缩略图信息
#[derive(Debug, Deserialize)]
pub struct ThumbnailInfo {
    /// 不同尺寸的缩略图
    pub thumbnails: Vec<Thumbnail>,
}

/// 缩略图
#[derive(Debug, Deserialize)]
pub struct Thumbnail {
    /// 宽度
    pub width: u32,
    /// 高度
    pub height: u32,
    /// 链接
    pub url: String,
}
