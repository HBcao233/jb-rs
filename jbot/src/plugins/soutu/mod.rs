mod buttons;

use std::sync::Arc;

use grammers_client::Client;
use grammers_client::media::Media;
use grammers_client::message::{Button, InputMessage, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::storages::SqliteSession;
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use tracing::error;

pub use self::buttons::SoutuButton;

const CUSTOM_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~')
    .remove(b'!')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')')
    .remove(b'/')
    .remove(b':');

#[crate::on_update]
async fn callback_handler(client: Client, update: Update, _session: Arc<SqliteSession>) {
    match update {
        Update::CallbackQuery(callback) => {
            let data = callback.data();

            if let Some(message_id) = SoutuButton::from_data(data) {
                return handle_soutu(callback, client, message_id).await;
            }
        }
        _ => {}
    }
}

async fn handle_soutu(callback: CallbackQuery, client: Client, message_id: i32) {
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| peer_id.to_ambient_ref());

    match client.get_messages_by_id(peer_ref, &[message_id]).await {
        Ok(messages) => {
            if let Some(Some(message)) = messages.first() {
                if let Some(media) = message.media() {
                    let filename = match media {
                        Media::Photo(ref photo) => format!("{}.jpg", photo.id()),
                        Media::Document(ref document) => {
                            let Some(name) = document.name() else {
                                error!("文件无文件名");
                                return;
                            };
                            match name.rsplitn(2, '.').next() {
                                Some(ext) => format!("{}.{}", document.id(), ext),
                                None => {
                                    error!(name, "文件无扩展名");
                                    return;
                                }
                            }
                        }
                        _ => {
                            error!(?media, "不支持的媒体");
                            return;
                        }
                    };
                    let path = crate::cache_dir().join(filename);

                    let exist = if tokio::fs::metadata(&path).await.is_ok() {
                        true
                    } else {
                        match client.download_media(&media, &path).await {
                            Ok(_) => true,
                            Err(e) => {
                                error!("媒体下载失败: {e}");
                                false
                            }
                        }
                    };

                    if exist {
                        match crate::plugins::hosting::get_url(path).await {
                            Ok(url) => {
                                let url = utf8_percent_encode(&url, CUSTOM_ENCODE_SET).to_string();

                                let reply_markup = ReplyMarkup::from_buttons(&[
                                    vec![Button::url(
                                        "GoogleLens",
                                        format!(
                                            "https://www.google.com/searchbyimage?client=app&udm=48&image_url={url}"
                                        ),
                                    )],
                                    vec![
                                        Button::url(
                                            "Yandex.ru",
                                            format!(
                                                "https://yandex.ru/images/search?url={url}&rpt=imageview"
                                            ),
                                        ),
                                        Button::url(
                                            "Yandex.com (锁国区)",
                                            format!(
                                                "https://yandex.com/images/search?url={url}&rpt=imageview"
                                            ),
                                        ),
                                    ],
                                    vec![
                                        Button::url(
                                            "SauceNAO",
                                            format!("https://saucenao.com/search.php?url={url}"),
                                        ),
                                        Button::url(
                                            "ascii2d",
                                            format!("https://ascii2d.net/search/url/{url}"),
                                        ),
                                        Button::url(
                                            "WAIT (动画)",
                                            format!("https://trace.moe/?auto&url={url}"),
                                        ),
                                    ],
                                    vec![
                                        Button::url("IQDB", format!("http://iqdb.org/?url={url}")),
                                        Button::url(
                                            "3D-IQDB",
                                            format!("http://3d.iqdb.org/?url={url}"),
                                        ),
                                        Button::url(
                                            "TinEye",
                                            format!("https://tineye.com/search?url={url}"),
                                        ),
                                        Button::url(
                                            "Bing",
                                            format!(
                                                "https://www.bing.com/images/search?q=imgurl:{url}&view=detailv2&iss=sbi"
                                            ),
                                        ),
                                    ],
                                ]);
                                match callback
                                    .answer()
                                    .respond(
                                        InputMessage::new()
                                            .text("请点击以下链接手动搜图")
                                            .reply_markup(reply_markup),
                                    )
                                    .await
                                {
                                    Ok(_) => return,
                                    Err(e) => {
                                        error!("回复结果失败: {e}")
                                    }
                                }
                            }
                            Err(e) => {
                                error!("获取外链失败: {e}");
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            error!("获取消息失败: {e}");
        }
    }

    let _ = callback.answer().send().await;
}
