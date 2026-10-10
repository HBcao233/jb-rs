//! Pixiv

pub mod types;

use std::path::PathBuf;

use tokio::fs;
use tracing::{error, info};
use wreq::{Client, StatusCode, header};

use self::types::{DETAIL_HOST, PixivDetails, PixivError, PixivResult, UgoiraMeta};
use crate::encode_cookies;

const LANG: &str = "zh";
const VERSION: &str = "55d9ace1031cc8b070a23db9df6c67552bd2ede6";

async fn fetch<T, K, V>(
    client: &Client,
    pid: &str,
    url: &str,
    query: &[(&str, String)],
    cookies: impl IntoIterator<Item = (K, V)>,
    cache_file: &PathBuf,
) -> Result<T, PixivError>
where
    T: serde::de::DeserializeOwned,
    K: AsRef<str>,
    V: AsRef<str>,
{
    let referer = format!("https://www.pixiv.net/artworks/{pid}");
    let cookie = encode_cookies(cookies);
    let response = client
        .get(url)
        .header(header::REFERER, referer)
        .header(header::COOKIE, cookie)
        .query(query)
        .send()
        .await?;
    let status = response.status();
    if status == StatusCode::NOT_FOUND {
        return Err(PixivError::NotFound);
    } else if status != StatusCode::OK {
        return Err(PixivError::Status(status.as_u16()));
    }

    let PixivResult {
        body,
        error,
        message,
    } = response.json().await?;
    if error {
        return Err(PixivError::Api(message));
    }

    let pretty = serde_json::to_string_pretty(&body)?;
    if let Err(e) = fs::write(&cache_file, &pretty).await {
        error!("缓存json文件失败: {e:?}");
    } else {
        info!("写入缓存: {}", cache_file.display());
    }

    Ok(serde_json::from_value(body)?)
}

/// 获取 Pixiv artwork 作品信息
pub async fn fetch_info<K, V>(
    client: &Client,
    pid: &str,
    cookies: impl IntoIterator<Item = (K, V)>,
    cache_file: &PathBuf,
) -> Result<PixivDetails, PixivError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let referer = format!("https://www.pixiv.net/artworks/{pid}");
    let query = [
        ("illust_id", pid.to_string()),
        ("ref", referer),
        ("lang", LANG.to_string()),
        ("version", VERSION.to_string()),
    ];

    fetch(client, pid, DETAIL_HOST, &query, cookies, cache_file).await
}

/// 获取动图信息
pub async fn fetch_ugoira_meta<K, V>(
    client: &Client,
    pid: &str,
    cookies: impl IntoIterator<Item = (K, V)>,
    cache_file: &PathBuf,
) -> Result<UgoiraMeta, PixivError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let url = format!("https://www.pixiv.net/ajax/illust/{pid}/ugoira_meta");
    fetch(client, pid, &url, &[], cookies, cache_file).await
}
