mod data_source;
mod tweet;

use std::env;
use std::sync::{Arc, OnceLock};

use anyhow::Context;
use grammers_client::Client;
use grammers_client::media::InputMedia;
use grammers_client::message::{Button, InputMessage, Message, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::storages::SqliteSession;
use grammers_session::types::{PeerKind, PeerRef};
use grammers_tl_types as tl;
use regex::regex;
use tracing::{error, info, warn};

use self::data_source::{get_tweet, parse_msg};
use crate::curl::stream_download;
use crate::database as db;

const HELP: &'static str = r#"推特解析，支持批量解析多条链接。
用法: /tid <url/tid> [url2 url3...]"#;

static CSRF_TOKEN: OnceLock<String> = OnceLock::new();
static AUTH_TOKEN: OnceLock<String> = OnceLock::new();

#[crate::on_setup]
fn setup() -> anyhow::Result<()> {
    let csrf_token = env::var("twitter_csrf_token")
        .context("environment variable \"twitter_csrf_token\" not found")?;
    let auth_token = env::var("twitter_auth_token")
        .context("environment variable \"twitter_auth_token\" not found")?;
    let _ = CSRF_TOKEN.set(csrf_token);
    let _ = AUTH_TOKEN.set(auth_token);
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

    let msg_id = message.id();
    let text = message.text();
    let mut matched = false;
    for (_, [tid]) in
        regex!(r"(?:https?://)?[a-z]*?(?:twitter|x)\.com/[a-zA-Z0-9_]+/status/(\d{13,20})")
            .captures_iter(text)
            .map(|c| c.extract())
    {
        matched = true;
        if let Err(e) = send_twitter(client.clone(), peer_ref, msg_id, tid.to_string()).await {
            error!("发送twitter失败: {e:?}");
        }
    }

    if !matched && text.starts_with("/tid") {
        for (_, [tid]) in regex!(r"(\d{13,20})")
            .captures_iter(text)
            .map(|c| c.extract())
        {
            matched = true;
            if let Err(e) = send_twitter(client.clone(), peer_ref, msg_id, tid.to_string()).await {
                error!("发送twitter失败: {e:?}");
            }
        }
    }

    if !matched && text.starts_with("/tid") {
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

#[crate::on_update]
async fn callback_handler(client: Client, update: Update, _session: Arc<SqliteSession>) {
    match update {
        Update::CallbackQuery(callback) => {
            let data = callback.data();
            if let Some(tid) = GetOriginalButton::from_data(data) {
                return send_original(client, callback, tid).await;
            }
        }
        _ => {}
    }
}

async fn send_twitter(
    client: Client,
    peer_ref: PeerRef,
    msg_id: i32,
    tid: String,
) -> anyhow::Result<()> {
    info!("tid: {tid}");

    let mid = client
        .send_message(
            peer_ref,
            InputMessage::new()
                .text(format!("[{tid}] 请等待..."))
                .reply_to(Some(msg_id)),
        )
        .await?;

    let wreq_client = crate::curl::get_client().build()?;
    let tweet = match get_tweet(&wreq_client, &tid).await {
        Ok(tweet) => tweet,
        Err(e) => {
            mid.edit(format!("[{tid}] {e}")).await?;
            return Ok(());
        }
    };

    let msg = parse_msg(&tweet);
    let mut medias = Vec::with_capacity(4);
    if let Some(entities) = &tweet.entities().media {
        let headers = Vec::new();
        let count = entities.len();
        for (index, media) in entities.into_iter().enumerate() {
            let media_type = media.r#type.as_str();
            let key = format!("{tid}_{}", index + 1);

            let cache = db::get_media(&key).await?;

            let mut input_media = if let Some(m) = cache {
                info!("使用已发送过的媒体: {key}");
                InputMedia::new().media(m)
            } else {
                mid.edit(format!("[{tid}] 媒体下载中 {} / {}...", index + 1, count))
                    .await?;

                let (ext, url, duration_millis) = match media_type {
                    "photo" => {
                        let url = &media.media_url_https;
                        let url = if url.contains('?') {
                            format!("{}&name=orig", url)
                        } else {
                            format!("{}?name=orig", url)
                        };
                        ("jpg", url, None)
                    }
                    "video" | "animated_gif" => {
                        let (video, duration_millis) = match &media.video_info {
                            Some(video_info) => (
                                video_info
                                    .variants
                                    .iter()
                                    .max_by_key(|v| {
                                        if v.content_type == "video/mp4" {
                                            v.bitrate.unwrap_or(0)
                                        } else {
                                            0
                                        }
                                    })
                                    .unwrap(),
                                video_info.duration_millis,
                            ),
                            None => {
                                error!("type={} 但 video_info 为空", media_type);
                                let _ = mid.edit("video_info 为空").await;
                                return Ok(());
                            }
                        };
                        ("mp4", video.url.clone(), duration_millis)
                    }
                    _ => {
                        let text = format!("暂不支持的媒体类型: {}", media_type);
                        error!("{}", text);
                        let _ = mid.edit(text).await;
                        return Ok(());
                    }
                };
                let name = format!("{key}.{ext}");

                let path = match stream_download(&wreq_client, &url, &name, &headers).await {
                    Ok(path) => path,
                    Err(e) => {
                        let tip = format!("[{tid}] 媒体 {} 下载失败", index + 1);
                        error!("{tip}: {e}");
                        mid.edit(tip).await?;
                        return Ok(());
                    }
                };

                mid.edit(format!("[{tid}] 媒体上传中 {} / {}...", index + 1, count))
                    .await?;
                let Ok(uploaded) = client.upload_file(&path).await else {
                    mid.edit(format!("[{tid}] 媒体 {} 上传失败", index + 1))
                        .await?;
                    return Ok(());
                };

                match media_type {
                    "photo" => InputMedia::new().mime_type("image/jpeg").photo(uploaded),
                    "video" | "animated_gif" => {
                        let thumb_url = &media.media_url_https;
                        let thumb_url = if thumb_url.contains('?') {
                            format!("{}&name=orig", thumb_url)
                        } else {
                            format!("{}?name=orig", thumb_url)
                        };
                        let thumb_name = format!("{key}_thumb.jpg");
                        let thumb =
                            match stream_download(&wreq_client, &thumb_url, &thumb_name, &headers)
                                .await
                            {
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

                        let duration = if let Some(millis) = duration_millis {
                            millis as f64 / 1000.0
                        } else {
                            match crate::ffmpeg::get_duration(&path).await {
                                Ok(d) => d,
                                Err(e) => {
                                    warn!("获取视频 ({}) 时长失败: {e}", path.display());
                                    0.0
                                }
                            }
                        };
                        let m = tl::types::InputMediaUploadedDocument {
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
                                    w: media.original_info.width,
                                    h: media.original_info.height,
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
                        };
                        InputMedia::new().media(m)
                    }
                    _ => {
                        warn!("不支持的媒体类型: {}", media_type);
                        continue;
                    }
                }
            };

            if index == 0 {
                input_media = input_media.html(&msg).reply_to(Some(msg_id));
            }
            medias.push(input_media)
        }
    }

    if medias.is_empty() {
        client
            .send_message(
                peer_ref,
                InputMessage::new().html(msg).reply_to(Some(msg_id)),
            )
            .await?;
    } else {
        let messages = match client.send_album(peer_ref, medias).await {
            Ok(m) => m,
            Err(e) => {
                let text = format!("[{tid}] 媒体发送失败");
                error!("{text}: {e:?}");
                mid.edit(text).await?;
                return Ok(());
            }
        };

        for (index, message) in messages.into_iter().enumerate() {
            let key = format!("{tid}_{}", index + 1);
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

        let reply_markup =
            ReplyMarkup::from_buttons_row(&[GetOriginalButton::new(tid.parse().unwrap()).raw]);
        if let Err(e) = client
            .send_message(
                peer_ref,
                InputMessage::new()
                    .text(format!("[{tid}] 解析完成"))
                    .reply_to(Some(msg_id))
                    .reply_markup(reply_markup),
            )
            .await
        {
            error!("发送消息失败: {e}");
        }
    }
    mid.delete().await?;

    Ok(())
}

pub struct GetOriginalButton {
    pub raw: Button,
}

impl GetOriginalButton {
    const ID: [u8; 4] = crate::id!("twitter_get_original");

    pub fn new(tid: u64) -> Self {
        let mut data = Vec::new();
        data.extend_from_slice(&Self::ID);
        data.extend_from_slice(&tid.to_le_bytes());
        let raw = Button::data("获取原图", data);
        Self { raw }
    }

    pub fn from_data(data: &[u8]) -> Option<u64> {
        if &data[..4] == Self::ID {
            let tid = u64::from_le_bytes(data[4..].try_into().unwrap());
            Some(tid)
        } else {
            None
        }
    }
}

async fn send_original(client: Client, callback: CallbackQuery, tid: u64) {
    info!("[send_original] tid: {}", &tid);

    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .ok()
        .and_then(|x| x)
        .unwrap_or_else(|| peer_id.to_ambient_ref());
    let mid = match client
        .send_message(
            peer_ref,
            InputMessage::new().text(format!("[{tid}] 请等待...")),
        )
        .await
    {
        Ok(m) => m,
        Err(e) => {
            error!("发送消息失败: {e}");
            let _ = callback.answer().alert("消息发送失败").send().await;
            return;
        }
    };

    let wreq_client = crate::curl::get_client().build().unwrap();
    let tweet = match get_tweet(&wreq_client, &tid.to_string()).await {
        Ok(tweet) => tweet,
        Err(e) => {
            let tip = format!("[{tid}] {e}");
            let _ = mid.edit(tip).await;
            let _ = callback.answer().send().await;
            return;
        }
    };

    let mut medias = Vec::new();
    if let Some(entities) = &tweet.entities().media {
        let headers = Vec::new();
        let count = entities.len();
        for (index, media) in entities.into_iter().enumerate() {
            let media_type = media.r#type.as_str();
            let key = format!("{tid}_{}_original", index + 1);

            let cache = db::get_media(&key).await.ok().and_then(|v| v);

            let input_media = if let Some(m) = cache {
                info!("使用已发送过的文件: {key}");
                m
            } else {
                let tip = format!("[{tid}] 媒体下载中 {} / {}...", index + 1, count);
                info!("{tip}");
                let _ = mid.edit(tip).await;

                let (ext, url, mime_type) = match media_type {
                    "photo" => {
                        let url = &media.media_url_https;
                        let url = if url.contains('?') {
                            format!("{}&name=orig", url)
                        } else {
                            format!("{}?name=orig", url)
                        };
                        ("jpg", url, "image/jpeg")
                    }
                    "video" => {
                        let video = media
                            .video_info
                            .as_ref()
                            .unwrap()
                            .variants
                            .iter()
                            .max_by_key(|v| {
                                if v.content_type == "video/mp4" {
                                    v.bitrate.unwrap_or(0)
                                } else {
                                    0
                                }
                            });
                        ("mp4", video.unwrap().url.clone(), "video/mp4")
                    }
                    _ => {
                        warn!("不支持的媒体类型: {}", media_type);
                        continue;
                    }
                };
                let name = format!("{key}.{ext}");

                let path = match stream_download(&wreq_client, &url, &name, &headers).await {
                    Ok(path) => path,
                    Err(e) => {
                        let tip = format!("[{tid}] 媒体 {} 下载失败", index + 1);
                        error!("{tip}: {e}");
                        let _ = mid.edit(tip).await;
                        let _ = callback.answer().send().await;
                        return;
                    }
                };

                let tip = format!("[{tid}] 媒体上传中 {} / {}...", index + 1, count);
                info!("{tip}");
                let _ = mid.edit(tip).await;
                let Ok(uploaded) = client.upload_file(path).await else {
                    let _ = mid
                        .edit(format!("[{tid}] 媒体 {} 上传失败", index + 1))
                        .await;
                    let _ = callback.answer().send().await;
                    return;
                };

                tl::types::InputMediaUploadedDocument {
                    nosound_video: true,
                    force_file: true,
                    spoiler: false,
                    file: uploaded.raw,
                    thumb: None,
                    mime_type: mime_type.to_string(),
                    attributes: vec![
                        tl::types::DocumentAttributeFilename { file_name: name }.into(),
                    ],
                    stickers: None,
                    ttl_seconds: None,
                    video_cover: None,
                    video_timestamp: None,
                }
                .into()
            };

            medias.push(InputMedia::new().media(input_media));
        }
    }

    if medias.is_empty() {
        let _ = client
            .send_message(peer_ref, InputMessage::new().text("该推文不存在媒体"))
            .await;
    } else {
        let messages = match client.send_album(peer_ref, medias).await {
            Ok(m) => m,
            Err(e) => {
                let text = format!("[{tid}] 媒体发送失败");
                error!("{text}: {e:?}");
                let _ = mid.edit(text).await;
                let _ = callback.answer().send().await;
                return;
            }
        };

        for (index, message) in messages.into_iter().enumerate() {
            let key = format!("{tid}_{}_original", index + 1);
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

    let _ = mid.delete().await;
    let _ = callback.answer().send().await;
}
