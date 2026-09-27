mod buttons;

use std::sync::Arc;

use grammers_client::Client;
use grammers_client::media::{InputMedia, Media};
use grammers_client::message::{InputMessage, Message, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::storages::SqliteSession;
use grammers_tl_types as tl;
use tracing::error;

pub use self::buttons::SwitchSpoilerButton;

const ADD_SPOILER: &str = "\u{01f648} 添加遮罩";
const REMOVE_SPOILER: &str = "\u{01f441} 移除遮罩";

#[crate::on_update]
async fn callback_handler(client: Client, update: Update, _session: Arc<SqliteSession>) {
    match update {
        Update::CallbackQuery(callback) => {
            let data = callback.data();

            if let Some((outgoing, message_ids)) = SwitchSpoilerButton::from_data(data) {
                return handle_switch_spoiler(callback, client, outgoing, &message_ids).await;
            }
        }
        _ => {}
    }
}

async fn handle_switch_spoiler(
    callback: CallbackQuery,
    client: Client,
    outgoing: bool,
    message_ids: &[i32],
) {
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| peer_id.to_ambient_ref());

    let messages: Vec<Message> = match client.get_messages_by_id(peer_ref, message_ids).await {
        Ok(messages) => messages.into_iter().flatten().collect(),
        Err(e) => {
            error!("获取消息失败: {e}");
            return;
        }
    };

    let Some(first_message) = messages.first() else {
        return;
    };
    let first_media = first_message.media().unwrap();
    let spoiler = match first_media {
        Media::Photo(p) => p.is_spoiler(),
        Media::Document(d) => d.is_spoiler(),
        _ => false,
    };
    let new_spoiler = !spoiler;

    if outgoing {
        for message in messages.iter() {
            if let Err(e) = message
                .edit(build_input_message(message, new_spoiler))
                .await
            {
                error!("编辑信息失败: {e}");
            }
        }

        match &callback.raw {
            tl::enums::Update::BotCallbackQuery(update) => {
                let reply_markup = ReplyMarkup::from_buttons_row(&[SwitchSpoilerButton::new(
                    true,
                    new_spoiler,
                    &message_ids,
                )]);
                let _ = client
                    .edit_message(
                        peer_ref,
                        update.msg_id,
                        InputMessage::new().reply_markup(reply_markup),
                    )
                    .await;
            }
            _ => {}
        }
        let _ = callback.answer().send().await;
    } else {
        let medias = messages
            .iter()
            .map(|m| build_input_media(&m, new_spoiler))
            .collect();
        match client.send_album(peer_ref, medias).await {
            Ok(messages) => {
                let message_ids: Vec<i32> = messages
                    .iter()
                    .filter_map(|message| message.as_ref().map(|m| m.id()))
                    .collect();
                let reply_markup = ReplyMarkup::from_buttons_row(&[SwitchSpoilerButton::new(
                    true,
                    new_spoiler,
                    &message_ids,
                )]);
                let message = InputMessage::new()
                    .text("操作完成")
                    .reply_markup(reply_markup);
                let _ = client.send_message(peer_ref, message).await;
            }
            Err(e) => {
                error!("发送媒体组失败: {e}");
            }
        }
        let _ = callback.answer().send().await;
    }
}

fn build_input_message(m: &Message, spoiler: bool) -> InputMessage {
    let mut input = InputMessage::new();
    if let Some(media) = m.media() {
        if let Some(mut raw) = media.to_raw_input_media() {
            match &mut raw {
                tl::enums::InputMedia::Photo(p) => {
                    p.spoiler = spoiler;
                }
                tl::enums::InputMedia::Document(d) => {
                    d.spoiler = spoiler;
                }
                _ => {}
            }
            input = input.media(raw);
        }
    }
    input
}

fn build_input_media(m: &Message, spoiler: bool) -> InputMedia {
    let mut input = InputMedia::new();
    if let Some(media) = m.media() {
        if let Some(mut raw) = media.to_raw_input_media() {
            match &mut raw {
                tl::enums::InputMedia::Photo(p) => {
                    p.spoiler = spoiler;
                }
                tl::enums::InputMedia::Document(d) => {
                    d.spoiler = spoiler;
                }
                _ => {}
            }
            input = input.media(raw);
        }
    }

    input = input.caption(m.text());
    if let Some(fmt_entities) = m.fmt_entities() {
        input = input.fmt_entities(fmt_entities.clone());
    }

    input
}
