use std::path::Path;

use futures_util::StreamExt;
use indicatif::{ProgressBar, ProgressStyle};
use tokio::fs;
use tokio::io::{self, AsyncWriteExt};
use tracing::{info, warn};
use wreq::header::HeaderValue;
use wreq::{Client, ClientBuilder};
use wreq_util::Emulation;

pub fn get_client() -> ClientBuilder {
    Client::builder().emulation(Emulation::Chrome137)
}

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("IO 错误: {0}")]
    Io(#[from] io::Error),

    #[error("状态码错误: {0}")]
    Status(u16),

    #[error("请求失败: {0}")]
    Http(#[from] wreq::Error),
}

pub async fn stream_download(
    client: &Client,
    url: &str,
    name: &str,
    headers: &[(&str, String)],
) -> Result<(), DownloadError> {
    let path = Path::new(".").join(name);
    let mut downloaded = match fs::metadata(&path).await {
        Ok(meta) if meta.is_file() => meta.len() as usize,
        _ => 0,
    };

    // 最多请求两次：第一次带 Range 尝试续传；若本地分片不可用，则删掉后从头再请求一次
    let (response, resume) = loop {
        let mut request = client.get(url);
        for (k, v) in headers {
            request = request.header(k.to_string(), v);
        }
        if downloaded > 0 {
            request = request.header("Range", format!("bytes={downloaded}-"));
        }
        let response = request.send().await?;
        if downloaded == 0 {
            break (response, false);
        }

        let status = response.status().as_u16();
        let (range_start, range_total) =
            parse_content_range(response.headers().get("content-range"));
        info!("downloaded: {}, range_total: {:?}", downloaded, range_total);
        match status {
            // 206：服务端从我们要求的位置返回了剩余部分，可以续传
            206 if range_start.map_or(true, |start| start == downloaded) => {
                break (response, true);
            }
            // 416：本地分片长度 >= 服务端文件长度。
            // 如果正好相等，说明上次其实已经下完了
            416 if range_total == Some(downloaded) => {
                return Ok(());
            }
            // 其他 206/416（起始位置或长度对不上）说明本地分片已不可信，删掉重来
            206 | 416 => {
                warn!("{name} 的缓存分片无法续传（HTTP {status}），将重新下载");
                fs::remove_file(&path).await?;
                downloaded = 0;
            }
            // 200：服务端不支持 Range，从头开始写；其他状态码交给下面统一报错
            _ => break (response, false),
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Err(DownloadError::Status(status.as_u16()));
    }

    let content_length = response
        .headers()
        .get("content-length")
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.parse().ok());

    let (mut file, total) = if resume {
        // 206 的 Content-Length 只是剩余部分的长度，总长度优先从 Content-Range 取
        let total = parse_content_range(response.headers().get("content-range"))
            .1
            .or_else(|| content_length.map(|len| downloaded + len));
        let file = fs::OpenOptions::new().append(true).open(&path).await?;
        (file, total)
    } else {
        downloaded = 0;
        (fs::File::create(&path).await?, content_length)
    };

    let bar = if let Some(t) = total {
        ProgressBar::new(t as u64)
    } else {
        ProgressBar::no_length()
    };
    bar.set_style(
        ProgressStyle::default_bar()
            .template(" [{wide_bar}] {binary_bytes}/{binary_total_bytes} ({percent}%)")
            .unwrap()
            .progress_chars("=> "),
    );
    bar.set_position(downloaded as u64);

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;

        file.write_all(&chunk).await?;

        bar.inc(chunk.len() as u64);
    }
    bar.finish();

    file.flush().await?;
    drop(file);

    Ok(())
}

/// 解析 `Content-Range: bytes <start>-<end>/<total>`（416 时为 `bytes */<total>`），
/// 返回 (start, total)，解析不出来的部分为 None
fn parse_content_range(content_range: Option<&HeaderValue>) -> (Option<usize>, Option<usize>) {
    let Some(value) = content_range.and_then(|v| v.to_str().ok()) else {
        return (None, None);
    };
    let value = value.trim().strip_prefix("bytes").unwrap_or(value).trim();
    let (range, total) = value.split_once('/').unwrap_or((value, ""));
    let start = range
        .split_once('-')
        .and_then(|(s, _)| s.trim().parse().ok());
    (start, total.trim().parse().ok())
}
