mod data_source;

use std::env;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use grammers_client::Client;
use grammers_client::media::InputMedia;
use grammers_client::message::{InputMessage, Message};
use grammers_session::types::{PeerKind, PeerRef};
use jb_core::pixiv::types::Manga;
use regex::regex;
use tracing::{error, info, warn};

use self::data_source::{get_info, parse_msg};
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
        if let Err(e) = message.reply(HELP).await {
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
    let manga_a = if let Some(manga_a) = info.illust_details.manga_a {
        manga_a
    } else {
        vec![Manga {
            page: 0,
            url_big: info.illust_details.url_big,
        }]
    };

    let count = info.illust_details.page_count.parse().unwrap_or(1);
    let mut medias = Vec::with_capacity(count);
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
    }

    match client.send_album(peer_ref, medias).await {
        Ok(messages) => {
            for (index, message) in messages.into_iter().enumerate() {
                let key = format!("{pid}_p{index}");
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

    mid.delete().await?;

    Ok(())
}
