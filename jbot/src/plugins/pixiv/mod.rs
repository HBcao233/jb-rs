mod data_source;

use std::env;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use grammers_client::Client;
use grammers_client::media::InputMedia;
use grammers_client::message::{InputMessage, Message};
use grammers_session::types::{PeerKind, PeerRef};
use grammers_tl_types as tl;
use jb_core::pixiv::types::{ImageResolution, Manga, UgoiraMeta};
use regex::regex;
use tokio::{fs, io};
use tracing::{error, info, warn};

use self::data_source::{
    create_ugoira_frames_txt, get_info, get_ugoira_meta, parse_msg, unzip_ugoira,
};
use crate::curl::stream_download;
use crate::database as db;

const HELP: &str = "Pixiv 解析。用法: /pixiv <url/pid>";

static PHPSESSID: OnceLock<Option<String>> = OnceLock::new();

#[crate::on_setup]
fn setup() -> anyhow::Result<()> {
    let key = env::var("pixiv_PHPSESSID").ok();
    if key.is_none() {
        warn!("Pixiv token were not provided, cannot fetch R-18 artworks.");
    }
    PHPSESSID
        .set(key)
        .expect("Must be empty before initialization.");

    Ok(())
}

#[crate::on_new_message]
async fn handler(client: Client, message: Arc<Message>) {
    if message.outgoing() {
        return;
    }

    let peer_id = message.peer_id();
    if peer_id.kind() != PeerKind::User {
        return;
    }

    let peer_ref = message
        .peer_ref()
        .await
        .ok()
        .and_then(|x| x)
        .unwrap_or_else(|| peer_id.to_ambient_ref());

    let re =
        regex!(r"(?:https?://)?(?:www\.)?(?:pixiv\.net/.*?(?:illust_id=|artworks/|i/))(\d{5,12})");

    let msg_id = message.id();
    let text = message.text().to_string();
    // info!("text: {text}");

    let starts_with_command = text.starts_with("/pid");

    let mut matched = false;
    if let Some(caps) = re.captures(&text) {
        let (_, [pid]) = caps.extract();
        info!("pid: {pid}");

        matched = true;
        if let Err(e) = send_pixiv(client.clone(), peer_ref, Some(msg_id), pid).await {
            error!(e, "send_pixiv failed");
        }
    }

    if !matched && starts_with_command {
        if let Some(caps) = regex!(r"(\d{5,12})").captures(&text) {
            let (_, [pid]) = caps.extract();
            info!("pid: {pid}");

            matched = true;
            if let Err(e) = send_pixiv(client.clone(), peer_ref, Some(msg_id), pid).await {
                error!(e, "send_pixiv failed");
            }
        }

        if !matched && let Err(e) = message.reply(HELP).await {
            error!("消息发送失败: {e}")
        }
    }
}

async fn send_pixiv(
    client: Client,
    peer_ref: PeerRef,
    msg_id: Option<i32>,
    pid: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let prefix = format!("[{pid}]");
    let mid = client
        .send_message(
            peer_ref,
            InputMessage::new()
                .text(format!("{prefix} 请等待..."))
                .reply_to(msg_id),
        )
        .await?;

    let wreq_client = crate::core::curl::get_client().build()?;
    let info = match get_info(&wreq_client, pid).await {
        Ok(info) => info,
        Err(e) => {
            error!("获取pixiv信息失败: {e}");
            mid.edit(format!("获取pixiv信息失败: {e}")).await?;
            return Ok(());
        }
    };

    let msg = parse_msg(&info);

    let headers = [("referer", format!("https://www.pixiv.net/artworks/{pid}"))];

    if info.illust_details.r#type == "2" {
        let key = format!("{pid}_ugoira");
        let media = if let Some(m) = db::get_media(&key).await? {
            info!("使用已发送过的媒体: {key}");
            m
        } else {
            let _ = mid.edit(format!("{prefix} 下载动图中...")).await;

            let UgoiraMeta {
                original_src,
                frames,
                ..
            } = get_ugoira_meta(&wreq_client, pid).await?;
            let ext = Path::new(&original_src)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let name = format!("{pid}_ugoira.{ext}");
            let path = match stream_download(&wreq_client, &original_src, &name, &headers).await {
                Ok(path) => path,
                Err(e) => {
                    let tip = format!("{prefix} 动图下载失败: {e}");
                    error!("{tip}");
                    mid.edit(tip).await?;
                    return Ok(());
                }
            };

            let pixiv_cache_dir = crate::cache_dir().join("pixiv/");

            let ugoira_dir = pixiv_cache_dir.join(format!("{pid}_ugoira"));
            let exist = match fs::metadata(&ugoira_dir).await {
                Ok(meta) => meta.is_dir(),
                Err(e) if e.kind() == io::ErrorKind::NotFound => false,
                Err(e) => {
                    error!("获取文件夹信息失败: {e}");
                    mid.edit("获取文件夹信息失败").await?;
                    return Ok(());
                }
            };
            if !exist {
                if let Err(e) = fs::create_dir(&ugoira_dir).await {
                    error!("解压目标文件夹创建失败: {e}");
                    mid.edit(format!("{prefix} 解压目标文件夹创建失败")).await?;
                    return Ok(());
                }
                if let Err(e) = unzip_ugoira(&path, &ugoira_dir).await {
                    error!("{prefix} 解压失败: {e}");
                    mid.edit(format!("{prefix} 解压失败")).await?;
                    return Ok(());
                }
            }

            let _ = mid.edit(format!("{prefix} 处理中...")).await;
            let frames_txt = pixiv_cache_dir.join(format!("{pid}_ugoira_frames.txt"));
            let duration = match create_ugoira_frames_txt(&frames, &ugoira_dir, &frames_txt).await {
                Ok(duration) => duration,
                Err(e) => {
                    error!("{prefix} 创建 frames 文件失败: {e}");
                    mid.edit(format!("{prefix} 创建 frames 文件失败")).await?;
                    return Ok(());
                }
            };
            let duration = duration as f64 / 1000.0;

            let target = pixiv_cache_dir.join(format!("{pid}_ugoira.mp4"));
            match crate::FFmpeg::new()
                .arg("-f")
                .arg("concat")
                .arg("-safe")
                .arg("0")
                .arg("-i")
                .arg(frames_txt)
                .arg("-c:v")
                .arg("h264")
                .arg("-vf")
                .arg("pad=ceil(iw/2)*2:ceil(ih/2)*2")
                .arg("-pix_fmt")
                .arg("yuv420p")
                .arg("-movflags")
                .arg("+faststart")
                .arg("-y")
                .arg(&target)
                .run()
                .await
            {
                Ok((status, stderr)) => {
                    if !status.success() {
                        error!("合成动图失败: {stderr}");
                        mid.edit(format!("{prefix} 合成动图失败")).await?;
                        return Ok(());
                    }
                }
                Err(e) => {
                    error!("合成动图失败: {e}");
                    mid.edit(format!("{prefix} 合成动图失败")).await?;
                    return Ok(());
                }
            }

            let _ = mid.edit(format!("{prefix} 上传中...")).await;
            let frame = ugoira_dir.join(&frames.first().unwrap().file);
            let thumb = match client.upload_file(frame).await {
                Ok(uploaded) => Some(uploaded.raw),
                Err(_) => None,
            };
            let uploaded = match client.upload_file(target).await {
                Ok(uploaded) => uploaded,
                Err(e) => {
                    error!("上传失败: {e}");
                    mid.edit(format!("{prefix} 上传失败")).await?;
                    return Ok(());
                }
            };
            let w = info.illust_details.width.parse().unwrap_or(200);
            let h = info.illust_details.height.parse().unwrap_or(200);

            tl::types::InputMediaUploadedDocument {
                nosound_video: true,
                force_file: false,
                spoiler: false,
                file: uploaded.raw,
                thumb,
                mime_type: "video/mp4".to_string(),
                attributes: vec![
                    tl::types::DocumentAttributeFilename { file_name: name }.into(),
                    tl::types::DocumentAttributeVideo {
                        round_message: false,
                        supports_streaming: true,
                        nosound: false,
                        duration,
                        w,
                        h,
                        preload_prefix_size: None,
                        video_start_ts: None,
                        video_codec: None,
                    }
                    .into(),
                ],
                stickers: None,
                ttl_seconds: None,
                video_cover: None,
                video_timestamp: None,
            }
            .into()
        };
        match client
            .send_message(
                peer_ref,
                InputMessage::new().html(msg).media(media).reply_to(msg_id),
            )
            .await
        {
            Ok(message) => match db::insert_from_message(&message, Some(&key)).await {
                Ok(_) => {
                    info!("添加缓存媒体: {key}");
                }
                Err(e) => {
                    error!("添加缓存媒体 {key} 失败: {e}");
                }
            },
            Err(e) => {
                error!("{prefix} 发送消息失败: {e}");
                mid.edit(format!("{prefix} 发送消息失败")).await?;
                return Ok(());
            }
        }

        mid.delete().await?;
        return Ok(());
    }

    let manga_a = if let Some(manga_a) = info.illust_details.manga_a {
        manga_a
    } else {
        if let Some(url_big) = info.illust_details.url_big {
            vec![Manga { page: 0, url_big }]
        } else {
            Vec::new()
        }
    };

    if manga_a.is_empty() {
        client
            .send_message(peer_ref, InputMessage::new().html(msg).reply_to(msg_id))
            .await?;
        mid.delete().await?;
        return Ok(());
    }

    // let count = info.illust_details.page_count.parse().unwrap_or(1);
    let count = manga_a.len();
    const BATCH_SIZE: usize = 10;
    let mut medias = Vec::with_capacity(BATCH_SIZE);
    let mut batch_pages = Vec::with_capacity(BATCH_SIZE);

    for (index, manga) in manga_a.into_iter().enumerate() {
        let human_index = index + 1;
        let page = manga.page;
        let url = manga.url_big;
        let key = format!("{pid}_p{page}");

        let mut media = if let Some(m) = db::get_media(&key).await? {
            info!("使用已发送过的媒体: {key}");
            InputMedia::new().media(m)
        } else {
            let ext = Path::new(&url)
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let mime_type = match ext.to_ascii_lowercase().as_str() {
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                _ => {
                    let text = format!("不支持的文件后缀: {ext:?}");
                    error!("{text}");
                    mid.edit(text).await?;
                    return Ok(());
                }
            };
            let name = format!("{key}.{ext}");

            let _ = mid
                .edit(format!("{prefix} 下载图片中 {human_index} / {count}..."))
                .await;

            let path = match stream_download(&wreq_client, &url, &name, &headers).await {
                Ok(path) => path,
                Err(e) => {
                    let tip = format!("{prefix} 图片 {human_index} 下载失败: {e}");
                    error!("{tip}");
                    mid.edit(tip).await?;
                    return Ok(());
                }
            };

            let path = match info.illust_details.illust_images.get(index) {
                Some(ImageResolution {
                    illust_image_width,
                    illust_image_height,
                }) if *illust_image_width > 2560 || *illust_image_height > 2560 => {
                    let resize_path = crate::cache_dir().join(format!("{key}_resize.{ext}"));
                    let scale = if illust_image_width > illust_image_height {
                        "scale=2560:-1"
                    } else {
                        "scale=-1:2560"
                    };
                    match crate::FFmpeg::new()
                        .arg("-i")
                        .arg(path)
                        .arg("-vf")
                        .arg(scale)
                        .arg("-y")
                        .arg(&resize_path)
                        .run()
                        .await
                    {
                        Ok((status, stderr)) => {
                            if !status.success() {
                                error!("缩放图片失败: {stderr}");
                                mid.edit(format!("{prefix} 缩放图片失败")).await?;
                                return Ok(());
                            }
                        }
                        Err(e) => {
                            error!("缩放图片失败: {e}");
                            mid.edit(format!("{prefix} 缩放图片失败")).await?;
                            return Ok(());
                        }
                    };
                    resize_path
                }
                _ => path,
            };

            let _ = mid
                .edit(format!("{prefix} 上传图片中 {human_index} / {count}..."))
                .await;
            let Ok(uploaded) = client.upload_file(path).await else {
                let tip = format!("{prefix} 图片 {human_index} 上传失败");
                error!("{tip}");
                mid.edit(tip).await?;
                return Ok(());
            };

            InputMedia::new().mime_type(mime_type).photo(uploaded)
        };

        if index == 0 {
            media = media.html(&msg).reply_to(msg_id);
        }
        medias.push(media);
        batch_pages.push(page);

        if medias.len() >= BATCH_SIZE || index == count - 1 {
            let chunk = std::mem::take(&mut medias);
            let pages = std::mem::take(&mut batch_pages);

            match client.send_album(peer_ref, chunk).await {
                Ok(messages) => {
                    for (message, page) in messages.into_iter().zip(pages.into_iter()) {
                        let key = format!("{pid}_p{}", page);
                        if let Some(m) = message {
                            match db::insert_from_message(&m, Some(&key)).await {
                                Ok(_) => {
                                    info!("添加缓存媒体: {key}");
                                }
                                Err(e) => {
                                    error!("添加缓存媒体 {key} 失败: {e}");
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("消息发送失败: {e}");
                    mid.edit("消息发送失败").await?;
                    return Ok(());
                }
            }
        }
    }

    mid.delete().await?;

    Ok(())
}
