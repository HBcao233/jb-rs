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

#[derive(Debug, thiserror::Error)]
pub enum YoutubeError {
    #[error("IO 错误: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("请求失败")]
    Http(#[from] wreq::Error),

    #[error("状态码错误: {0}")]
    Status(u16),

    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Youtube 视频不存在")]
    NotFound,

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

#[derive(Debug, Deserialize)]
pub struct VideoDetails {
    pub author: String,
    #[serde(rename = "channelId")]
    pub channel_id: String,
    #[serde(rename = "isCrawlable")]
    pub is_crawlable: bool,
    #[serde(rename = "lengthSeconds")]
    pub length_seconds: String,
    #[serde(rename = "shortDescription")]
    pub short_description: String,
    pub thumbnail: ThumbnailInfo,
    pub title: String,

    #[serde(rename = "videoId")]
    pub video_id: String,
}

#[derive(Debug, Deserialize)]
pub struct ThumbnailInfo {
    pub thumbnails: Vec<Thumbnail>,
}

#[derive(Debug, Deserialize)]
pub struct Thumbnail {
    pub width: u32,
    pub height: u32,
    pub url: String,
}
