mod abogus;
mod data_source;
mod types;

use std::sync::Arc;

use grammers_client::Client;
use grammers_client::media::InputMedia;
use grammers_client::message::{InputMessage, Message};
use grammers_session::types::{PeerKind, PeerRef};
use grammers_tl_types as tl;
use regex::regex;
use tracing::{error, info, warn};
use wreq::redirect::Policy;

use self::data_source::{get_aweme_detail, parse_msg};
use crate::curl::{stream_download, stream_download_with_callback};
use crate::database as db;
use crate::progress::{Progress, ProgressScheduler, ProgressStyle};
use crate::utils::upload_file_with_callback;

const HELP: &str = "Bilibili 解析。用法: /bili <url>";

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

    let re = regex!(
        r"(?:https?://)?(?:www\.|so\.)?(?:ies)?douyin\.com/(?:.*?video/|(?:share/)?note/|.*?modal_id=|.*?actv_aid=)?([0-9]{12,20})"
    );
    let short_re = regex!(r"(?:https?://)?v\.douyin\.com/([0-9a-zA-Z_-]{5,14})");

    let msg_id = message.id();
    let mut text = message.text().to_string();
    let starts_with_command = text.starts_with("/douyin");

    let mut short_matched = false;
    if let Some(caps) = short_re.captures(&text) {
        let (_, [short_id]) = caps.extract();
        info!("v.douyin.com: {short_id}");
        let url = format!("https://v.douyin.com/{}", short_id);
        let wreq_client = crate::curl::get_client().build().unwrap();
        let response = match wreq_client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                error!("v.douyin.com 请求失败: {e}");
                if let Err(e) = message.reply("短链解析失败").await {
                    error!("消息发送失败: {e}");
                }
                return;
            }
        };
        if let Some(location) = response
            .headers()
            .get("location")
            .and_then(|l| l.to_str().ok())
        {
            text = location.to_string();
            short_matched = true;
        } else {
            if let Err(e) = message.reply("短链解析失败").await {
                error!("消息发送失败: {e}");
            }
            return;
        }
    }

    let mut matched = false;
    if let Some(caps) = re.captures(&text) {
        let (_, [aid]) = caps.extract();
        info!("input: {aid}");

        if short_matched {
            let _ = message
                .reply(format!("https://www.douyin.com/video/{aid}"))
                .await;
        }

        matched = true;
        if let Err(e) = send_douyin(client.clone(), peer_ref, msg_id, aid).await {
            error!("发送douyin失败: {e:?}");
        }
    }

    if !matched && starts_with_command {
        if let Err(e) = client
            .send_message(
                peer_ref,
                InputMessage::new().text(HELP).reply_to(Some(msg_id)),
            )
            .await
        {
            error!("发送帮助信息失败: {e:?}");
        }
    }
}

async fn send_douyin(
    client: Client,
    peer_ref: PeerRef,
    msg_id: i32,
    aid: &str,
) -> anyhow::Result<()> {
    let prefix = format!("[douyin/{aid}]");
    let mid = client
        .send_message(
            peer_ref,
            InputMessage::new()
                .text(format!("{prefix} 请等待..."))
                .reply_to(Some(msg_id)),
        )
        .await?;
    let mut mid = Arc::new(mid);

    let wreq_client = crate::curl::get_client().build()?;
    let detail = match get_aweme_detail(&wreq_client, aid).await {
        Ok(d) => d,
        Err(e) => {
            error!("{prefix} 获取Aweme失败: {e}");
            mid.edit(format!("{prefix} {e}")).await?;
            return Ok(());
        }
    };
    // let aid = detail.aweme_id.clone();
    let msg = parse_msg(&detail);
    // let aweme_type = detail.aweme_type;
    // info!("aweme_type: {aweme_type}");

    let down_client = crate::curl::get_client()
        .redirect(Policy::limited(5))
        .build()?;
    let headers = vec![("referer", "https://www.douyin.com/".to_string())];

    if detail.images.is_none()
        && let Some(video) = detail.video
    {
        let thumb_url = video.origin_cover.url_list.last().unwrap();
        let thumb_name = format!("{aid}_thumb.jpg");
        let thumb = match stream_download(&down_client, thumb_url, &thumb_name, &headers).await {
            Ok(path) => match client.upload_file(path).await {
                Ok(uploaded) => Some(uploaded.raw),
                Err(e) => {
                    warn!("上传 {thumb_name} 失败: {e}");
                    None
                }
            },
            Err(e) => {
                warn!("下载 {thumb_name} 失败: {e}");
                None
            }
        };

        let bar = ProgressScheduler::new(Progress::new(Arc::clone(&mid)));
        let w = video.play_addr.width;
        let h = video.play_addr.height;
        let duration = video.duration as f64 / 1000.0;

        let key = format!("douyin_{aid}_play");
        let url = video.play_addr.url_list.last().unwrap();
        if let Err(e) = send_video(
            &client,
            peer_ref,
            msg_id,
            &prefix,
            &msg,
            Arc::clone(&mid),
            &bar,
            &down_client,
            thumb.clone(),
            &headers,
            w,
            h,
            duration,
            key,
            url,
        )
        .await
        {
            error!("发送视频失败: {e}");
        }

        mid.delete().await?;
        mid = Arc::new(
            client
                .send_message(
                    peer_ref,
                    InputMessage::new()
                        .text(format!("{prefix} 请等待..."))
                        .reply_to(Some(msg_id)),
                )
                .await?,
        );

        if let Some(download_addr) = video.download_addr {
            let key = format!("douyin_{aid}_down");
            let url = download_addr.url_list.last().unwrap();
            if let Err(e) = send_video(
                &client,
                peer_ref,
                msg_id,
                &prefix,
                &msg,
                Arc::clone(&mid),
                &bar,
                &down_client,
                thumb.clone(),
                &headers,
                w,
                h,
                duration,
                key,
                url,
            )
            .await
            {
                error!("发送视频失败: {e}");
            }
        }
    }

    mid.delete().await?;

    if let Some(images) = detail.images {
        let mid = client
            .send_message(
                peer_ref,
                InputMessage::new()
                    .text(format!("{prefix} 请等待..."))
                    .reply_to(Some(msg_id)),
            )
            .await?;

        let count = images.len();
        let mut medias = Vec::with_capacity(count);
        for (index, image) in images.iter().enumerate() {
            let key = format!("douyin_image_{aid}_p{}", index);
            let mut media = if let Some(m) = db::get_media(&key).await? {
                info!("使用已发送过的媒体: {key}");
                InputMedia::new().media(m)
            } else {
                let name = format!("{key}.jpeg");
                let url = image.url_list.last().unwrap();
                mid.edit(format!("{prefix} 下载图片中 {} / {}...", index + 1, count))
                    .await?;

                let path = match stream_download(&wreq_client, &url, &name, &headers).await {
                    Ok(path) => path,
                    Err(e) => {
                        let tip = format!("{prefix} 图片 {} 下载失败: {e}", index + 1);
                        error!("{tip}");
                        mid.edit(tip).await?;
                        return Ok(());
                    }
                };

                mid.edit(format!("{prefix} 上传图片中 {} / {}...", index + 1, count))
                    .await?;
                let Ok(uploaded) = client.upload_file(path).await else {
                    let tip = format!("{prefix} 图片 {} 上传失败", index + 1);
                    error!("{tip}");
                    mid.edit(tip).await?;
                    return Ok(());
                };

                InputMedia::new().mime_type("image/jpeg").photo(uploaded)
            };

            if index == 0 {
                media = media.html(&msg).reply_to(Some(msg_id));
            }
            medias.push(media)
        }

        match client.send_album(peer_ref, medias).await {
            Ok(messages) => {
                for (index, message) in messages.into_iter().enumerate() {
                    let key = format!("douyin_image_{aid}_p{}", index);
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
                let tip = format!("{prefix} 媒体发送失败");
                error!("{tip}: {e:?}");
                mid.edit(tip).await?;
                return Ok(());
            }
        }

        mid.delete().await?;
    }

    Ok(())
}

async fn send_video(
    client: &Client,
    peer_ref: PeerRef,
    msg_id: i32,
    prefix: &str,
    msg: &str,
    mid: Arc<Message>,
    bar: &ProgressScheduler,
    wreq_client: &wreq::Client,
    thumb: Option<tl::enums::InputFile>,
    headers: &[(&str, String)],
    w: i32,
    h: i32,
    duration: f64,
    key: String,
    url: &str,
) -> anyhow::Result<()> {
    let media = if let Some(m) = db::get_media(&key).await? {
        m
    } else {
        let name = format!("{key}.mp4");

        let p = format!("{prefix} 下载视频中...");
        info!("{p}");
        bar.style(ProgressStyle::Size);
        bar.prefix(&p);
        mid.edit(p).await?;
        let path = match stream_download_with_callback(
            &wreq_client,
            url,
            &name,
            &headers,
            |downloaded, total| {
                bar.sync_update(downloaded, total);
            },
        )
        .await
        {
            Ok(path) => path,
            Err(e) => {
                let tip = format!("{prefix} 视频下载失败: {e}");
                error!("{tip}");
                mid.edit(tip).await?;
                return Ok(());
            }
        };

        let p = format!("{prefix} 上传中...");
        info!("{p}");
        bar.style(ProgressStyle::Size);
        bar.prefix(&p);
        mid.edit(p).await?;
        let Ok(uploaded) = upload_file_with_callback(&client, path, |uploaded, total| {
            bar.sync_update(uploaded, total);
        })
        .await
        else {
            mid.edit(format!("{prefix} 上传失败")).await?;
            return Ok(());
        };

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
            InputMessage::new()
                .html(msg)
                .media(media)
                .reply_to(Some(msg_id)),
        )
        .await
    {
        Ok(message) => match db::insert_from_message(&message, Some(&key)).await {
            Ok(_) => {
                info!("添加缓存视频: {key}");
            }
            Err(e) => {
                error!("添加缓存视频 {key} 失败: {e}");
            }
        },
        Err(e) => {
            error!("消息发送失败: {e}");
        }
    }

    Ok(())
}
