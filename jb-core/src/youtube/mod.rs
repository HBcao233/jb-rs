//! Youtube

pub mod types;

use std::path::PathBuf;

use tokio::fs;
use tracing::{error, info, warn};
use wreq::{Client, StatusCode, header};

use self::types::{
    CLIENT_NAME, CLIENT_VERSION, INFO_HOST, InfoBody, InfoResult, PlayabilityStatus, VideoDetails,
    YoutubeError, Ytcfg,
};

async fn get_ytcfg(client: &Client) -> Ytcfg {
    let Ok(response) = client
        .get("https://www.youtube.com/")
        .header("X-YouTube-Client-Name", CLIENT_NAME)
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .header(header::ORIGIN, "https://www.youtube.com")
        .send()
        .await
    else {
        return Ytcfg::default();
    };
    let Ok(text) = response.text().await else {
        return Ytcfg::default();
    };
    if let Some(pos1) = text.find("ytcfg.set({")
        && let Some(pos2) = text.find("}); ")
    {
        match text.get((pos1 + 10)..(pos2 + 1)) {
            Some(cfg) => serde_json::from_str(cfg).unwrap_or_default(),
            None => Ytcfg::default(),
        }
    } else {
        Ytcfg::default()
    }
}

/// 获取 Youtube 视频信息
pub async fn get_video_info(
    client: &Client,
    video_id: &str,
    cache_file: PathBuf,
) -> Result<VideoDetails, YoutubeError> {
    let cache: Option<serde_json::Value> = match fs::read_to_string(&cache_file).await {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(e) => {
                warn!("缓存解析失败: {e}");
                None
            }
        },
        Err(_) => None,
    };
    let data = if let Some(data) = cache {
        info!("使用缓存: {}", cache_file.display());
        data
    } else {
        let ytcfg = get_ytcfg(client).await;
        let body = InfoBody::new(video_id.to_string());

        let response = client
            .post(INFO_HOST)
            .header("X-YouTube-Client-Name", CLIENT_NAME)
            .header("X-YouTube-Client-Version", CLIENT_VERSION)
            .header(header::ORIGIN, "https://www.youtube.com")
            .header("X-Goog-Visitor-Id", ytcfg.VISITOR_DATA)
            .json(&body)
            .send()
            .await?;

        let status = response.status();
        if status != StatusCode::OK {
            return Err(YoutubeError::Status(status.as_u16()));
        }

        let data: serde_json::Value = response.json().await?;
        let pretty = serde_json::to_string_pretty(&data)?;
        if let Err(e) = fs::write(&cache_file, &pretty).await {
            error!("缓存json文件失败: {e:?}");
        } else {
            info!("写入缓存: {}", cache_file.display());
        }

        data
    };

    let InfoResult {
        playabilityStatus: PlayabilityStatus { status, reason },
        videoDetails,
    } = serde_json::from_value(data)?;

    match status.as_str() {
        "ERROR" | "LOGIN_REQUIRED" => Err(YoutubeError::Api(reason)),
        _ => Ok(videoDetails),
    }
}
