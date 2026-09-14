use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use wreq::header::HeaderValue;
use wreq_util::Emulation::Chrome137;

fn empty_callback(_downloaded: usize, _total: Option<usize>) {}

pub fn get_client() -> wreq::ClientBuilder {
    wreq::Client::builder().emulation(Chrome137)
}

pub async fn stream_download(
    client: &wreq::Client,
    url: String,
    name: &str,
    headers: &[(&str, String)],
) -> anyhow::Result<PathBuf> {
    stream_download_with_callback(client, url, name, headers, empty_callback).await
}

pub async fn stream_download_with_callback<F>(
    client: &wreq::Client,
    url: String,
    name: &str,
    headers: &[(&str, String)],
    progress_callback: F,
) -> anyhow::Result<PathBuf>
where
    F: Fn(usize, Option<usize>) -> (),
{
    let cache_dir = Path::new("cache");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        log::error!("缓存文件夹创建失败: {e:?}");
    }

    // 只有下载完成的文件才会用最终文件名，存在即代表完整
    let path = cache_dir.join(name);
    if path.is_file() {
        return Ok(path);
    }

    // 下载中的数据写到 .part 文件，完成后再重命名，以此区分"完整"和"下了一半"
    let part_path = cache_dir.join(format!("{name}.part"));
    let mut downloaded = match fs::metadata(&part_path).await {
        Ok(meta) if meta.is_file() => meta.len() as usize,
        _ => 0,
    };

    // 最多请求两次：第一次带 Range 尝试续传；若本地分片不可用，则删掉后从头再请求一次
    let (response, resume) = loop {
        let mut request = client.get(&url);
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
        log::info!("downloaded: {}, range_total: {:?}", downloaded, range_total);
        match status {
            // 206：服务端从我们要求的位置返回了剩余部分，可以续传
            206 if range_start.map_or(true, |start| start == downloaded) => {
                break (response, true);
            }
            // 416：本地分片长度 >= 服务端文件长度。
            // 如果正好相等，说明上次其实已经下完了
            416 if range_total == Some(downloaded) => {
                fs::rename(&part_path, &path).await?;
                progress_callback(downloaded, Some(downloaded));
                return Ok(path);
            }
            // 其他 206/416（起始位置或长度对不上）说明本地分片已不可信，删掉重来
            206 | 416 => {
                log::warn!("{name} 的缓存分片无法续传（HTTP {status}），将重新下载");
                fs::remove_file(&part_path).await?;
                downloaded = 0;
            }
            // 200：服务端不支持 Range，从头开始写；其他状态码交给下面统一报错
            _ => break (response, false),
        }
    };

    let status = response.status();
    if !status.is_success() {
        return Err(anyhow::anyhow!(format!(
            "下载失败，HTTP 状态码：{}",
            status,
        )));
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
        let file = fs::OpenOptions::new().append(true).open(&part_path).await?;
        (file, total)
    } else {
        downloaded = 0;
        (fs::File::create(&part_path).await?, content_length)
    };

    // 先回调一次，让调用方立刻看到续传前已有的进度
    progress_callback(downloaded, total);

    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;

        file.write_all(&chunk).await?;

        downloaded += chunk.len();
        progress_callback(downloaded, total);
    }

    file.flush().await?;
    drop(file);

    // 连接被中途掐断时 stream 也可能"正常"结束，校验长度避免把半个文件当成完整文件；
    // 失败时保留 .part，下次调用会自动续传
    if let Some(total) = total {
        if downloaded != total {
            anyhow::bail!("下载不完整（{downloaded}/{total} 字节），下次调用将自动续传");
        }
    }

    fs::rename(&part_path, &path).await?;
    Ok(path)
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
