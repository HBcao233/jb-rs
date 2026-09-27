mod buttons;
mod database;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use grammers_client::Client;
use grammers_client::media::InputMedia;
use grammers_client::message::{InputMessage, Message, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::storages::SqliteSession;
use grammers_session::types::PeerRef;
use tracing::{error, info};

pub use self::buttons::{AddMergeButton, DirectMergeButton, FinishMergeButton};

#[crate::on_update]
async fn callback_handler(client: Client, update: Update, _session: Arc<SqliteSession>) {
    match update {
        Update::CallbackQuery(callback) => {
            let data = callback.data();
            if let Some(message_ids) = AddMergeButton::from_data(data) {
                return handle_add_merge(callback, message_ids).await;
            }
            if let Some(_) = FinishMergeButton::from_data(data) {
                return handle_finish_merge(callback, client).await;
            }
            if let Some(message_ids) = DirectMergeButton::from_data(data) {
                return handle_direct_merge(callback, client, &message_ids).await;
            }
        }
        _ => {}
    }
}

async fn handle_add_merge(callback: CallbackQuery, message_ids: Vec<i32>) {
    let peer_id = callback.peer_id();
    info!("{:?}", &message_ids);

    let count = message_ids.len();
    let text = format!("已添加 {count} 条媒体");
    if let Err(e) = database::insert_session(callback.peer_id(), message_ids).await {
        error!("添加合并失败 {e}");
        if let Err(e) = callback.answer().alert("添加合并媒体失败").send().await {
            error!("回复失败按钮回调失败: {e}");
        }
    } else {
        let reply_markup = ReplyMarkup::from_buttons(&[vec![FinishMergeButton::new()]]);
        match callback
            .answer()
            .respond(InputMessage::new().text(text).reply_markup(reply_markup))
            .await
        {
            Ok(message) => {
                if let Err(e) = message.pin().await {
                    error!("置顶消息失败: {e}");
                }
                if let Err(e) = database::insert_pinned(peer_id, message.id()).await {
                    error!("记录置顶消息失败: {e}");
                }
            }
            Err(e) => error!("回复失败按钮回调失败: {e}"),
        }
    }
}

async fn handle_finish_merge(callback: CallbackQuery, client: Client) {
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| peer_id.to_ambient_ref());

    match database::get_session(peer_id).await {
        Ok(message_ids) => {
            let want = message_ids.len();
            let success_count = send_message_ids(&client, peer_ref, &message_ids).await;

            let text = format!("已成功合并 {} / {} 条媒体", success_count, want);
            if let Err(e) = callback.answer().respond(text).await {
                error!("回复失败按钮回调失败: {e}");
            }
        }
        Err(e) => {
            error!("获取合并记录失败: {e}");
        }
    }

    match database::get_pinned(peer_id).await {
        Ok(pinned) => {
            if let Err(e) = client.delete_messages(peer_ref, &pinned).await {
                error!("删除消息失败: {e}");
            }
        }
        Err(e) => {
            error!("获取置顶消息失败: {e}");
        }
    }

    if let Err(e) = database::finish_session(peer_id).await {
        error!("完成合并失败: {e}")
    }
}

async fn handle_direct_merge(callback: CallbackQuery, client: Client, message_ids: &[i32]) {
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or(None)
        .unwrap_or_else(|| peer_id.to_ambient_ref());
    let want = message_ids.len();
    let success_count = send_message_ids(&client, peer_ref, message_ids).await;

    let text = format!("已成功合并 {} / {} 条媒体", success_count, want);
    if let Err(e) = callback.answer().respond(text).await {
        error!("回复失败按钮回调失败: {e}");
    }
}

async fn send_message_ids(client: &Client, peer_ref: PeerRef, message_ids: &[i32]) -> usize {
    let mut success_count: usize = 0;

    let uniq_ids: Vec<i32> = {
        let mut seen = HashSet::new();
        message_ids
            .iter()
            .copied()
            .filter(|id| seen.insert(*id))
            .collect()
    };
    let mut cache: HashMap<i32, Message> = HashMap::with_capacity(uniq_ids.len());
    for chunk in uniq_ids.chunks(100) {
        match client.get_messages_by_id(peer_ref, chunk).await {
            Ok(messages) => {
                for m in messages.into_iter().flatten() {
                    cache.insert(m.id(), m);
                }
            }
            Err(e) => error!("获取消息失败: {e}"),
        }
    }

    for chunk in message_ids.chunks(10) {
        let medias: Vec<InputMedia> = chunk
            .iter()
            .filter_map(|id| cache.get(id))
            .map(build_input_media)
            .collect();
        if medias.is_empty() {
            continue;
        }

        let n = medias.len();
        match client.send_album(peer_ref, medias).await {
            Ok(_) => {
                success_count += n;
            }
            Err(e) => error!("发送合并媒体失败: {e}"),
        }
    }

    success_count
}

fn build_input_media(m: &Message) -> InputMedia {
    let mut input = InputMedia::new().caption(m.text());
    if let Some(media) = m.media() {
        if let Some(raw) = media.to_raw_input_media() {
            input = input.media(raw);
        }
    }
    if let Some(fmt_entities) = m.fmt_entities() {
        input = input.fmt_entities(fmt_entities.clone());
    }
    input
}
