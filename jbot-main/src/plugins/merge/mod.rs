mod database;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use grammers_client::Client;
use grammers_client::media::InputMedia;
use grammers_client::message::{Button, InputMessage, Message, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::types::{PeerKind, PeerRef};

#[crate::on_grouped_messages]
async fn messages_handler(client: Client, messages: Vec<Arc<Message>>) {
    let message = messages.first().unwrap();
    if message.outgoing() {
        return;
    }
    let peer_id = message.peer_id();
    if peer_id.kind() != PeerKind::User {
        return;
    }

    if let Ok(Some(peer_ref)) = message.peer_ref().await {
        if let Some(media) = message.media() {
            if crate::utils::can_grouped(&media) {
                send_merge_button(client.clone(), peer_ref, &messages).await;
            }
        }
    }
}

#[crate::on_update]
async fn callback_handler(client: Client, update: Update) {
    match update {
        Update::CallbackQuery(callback) => {
            let data = callback.data();
            if let Some(message_ids) = AddMergeButton::from_data(data) {
                return handle_add_merge(callback, message_ids).await;
            }
            if let Some(_) = FinishMergeButton::from_data(data) {
                return handle_finish_merge(callback, client).await;
            }
        }
        _ => {}
    }
}

pub struct AddMergeButton {
    pub raw: Button,
}

impl AddMergeButton {
    const ID: [u8; 4] = crate::id!("add_merge");

    pub fn new(message_ids: &[i32]) -> Self {
        if message_ids.len() > 10 {
            panic!("message_ids 长度不能大于 10");
        }

        let mut data = Vec::with_capacity(44);
        data.extend_from_slice(&Self::ID);
        for &num in message_ids {
            data.extend_from_slice(&num.to_le_bytes());
        }
        let raw = Button::data("合并媒体", data);
        Self { raw }
    }

    pub fn from_data(data: &[u8]) -> Option<Vec<i32>> {
        if &data[..4] == Self::ID {
            let res: Vec<_> = data[4..]
                .chunks_exact(4)
                .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();
            if res.len() > 10 {
                panic!("一次添加数量不可能大于 10");
            }
            Some(res)
        } else {
            None
        }
    }
}

async fn send_merge_button(client: Client, peer: PeerRef, messages: &[Arc<Message>]) {
    let text = format!("收到 {} 条媒体", messages.len());
    let message_ids: Vec<i32> = messages.iter().map(|m| m.id()).collect();
    let reply_markup = ReplyMarkup::from_buttons(&[vec![AddMergeButton::new(&message_ids).raw]]);
    if let Err(e) = client
        .send_message(
            peer,
            InputMessage::new()
                .text(text)
                .reply_to(message_ids.first().copied())
                .reply_markup(reply_markup),
        )
        .await
    {
        log::error!("合并button发送失败: {e}");
    }
}

pub struct FinishMergeButton {
    pub raw: Button,
}

impl FinishMergeButton {
    const ID: [u8; 4] = crate::id!("finish_merge");

    pub fn new() -> Self {
        let raw = Button::data("完成合并", Self::ID);
        Self { raw }
    }

    pub fn from_data(data: &[u8]) -> Option<()> {
        if &data[..4] == Self::ID {
            Some(())
        } else {
            None
        }
    }
}

async fn handle_add_merge(callback: CallbackQuery, message_ids: Vec<i32>) {
    let peer_id = callback.peer_id();
    log::info!("{:?}", &message_ids);

    let count = message_ids.len();
    let text = format!("已添加 {count} 条媒体");
    if let Err(e) = database::insert_session(callback.peer_id(), message_ids).await {
        log::error!("添加合并失败 {e}");
        if let Err(e) = callback.answer().alert("添加合并媒体失败").send().await {
            log::error!("回复失败按钮回调失败: {e}");
        }
    } else {
        let reply_markup = ReplyMarkup::from_buttons(&[vec![FinishMergeButton::new().raw]]);
        match callback
            .answer()
            .respond(InputMessage::new().text(text).reply_markup(reply_markup))
            .await
        {
            Ok(message) => {
                if let Err(e) = message.pin().await {
                    log::error!("置顶消息失败: {e}");
                }
                if let Err(e) = database::insert_pinned(peer_id, message.id()).await {
                    log::error!("记录置顶消息失败: {e}");
                }
            }
            Err(e) => log::error!("回复失败按钮回调失败: {e}"),
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
                    Err(e) => log::error!("获取消息失败: {e}"),
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
                    Err(e) => log::error!("发送合并媒体失败: {e}"),
                }
            }

            let text = format!("已成功合并 {} / {} 条媒体", success_count, want);
            if let Err(e) = callback.answer().respond(text).await {
                log::error!("回复失败按钮回调失败: {e}");
            }
        }
        Err(e) => {
            log::error!("获取合并记录失败: {e}");
        }
    }

    match database::get_pinned(peer_id).await {
        Ok(pinned) => {
            if let Err(e) = client.delete_messages(peer_ref, &pinned).await {
                log::error!("删除消息失败: {e}");
            }
        }
        Err(e) => {
            log::error!("获取置顶消息失败: {e}");
        }
    }

    if let Err(e) = database::finish_session(peer_id).await {
        log::error!("完成合并失败: {e}")
    }
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
