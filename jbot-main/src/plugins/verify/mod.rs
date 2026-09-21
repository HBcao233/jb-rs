mod database;

use std::sync::Arc;

use database::{VerifyStatus, get_status, set_status};
use grammers_client::Client;
use grammers_client::message::{Button, InputMessage, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::Session;
use grammers_session::storages::SqliteSession;
use grammers_session::types::{PeerId, PeerRef};
use grammers_tl_types as tl;
use jiff::{SignedDuration, Timestamp};
use rand::prelude::IteratorRandom;
use rand::{random_range, rng};

use crate::plugins::group_config::{CONFIGS, Config};

const VERIFY_LIMIT: SignedDuration = SignedDuration::from_mins(3);

const VERIFY_ENABLED_KEY: &str = "verify_enabled";

const EMOJIS: [char; 13] = [
    '\u{2764}',   // ❤
    '\u{01f970}', // 🥰
    '\u{01f975}', // 🥵
    '\u{01f32d}', // 🌭
    '\u{01f95b}', // 🥛
    '\u{01f346}', // 🍆
    '\u{01f525}', // 🔥
    '\u{26a1}',   // ⚡
    '\u{01f973}', // 🥳
    '\u{01f33f}', // 🌿
    '\u{2b50}',   // ⭐
    '\u{01f4a6}', // 💦
    '\u{01f365}', // 🍥
];

#[linkme::distributed_slice(CONFIGS)]
static CONFIG: Config = Config {
    key: VERIFY_ENABLED_KEY,
    name: "入群验证",
};

#[crate::on_update]
async fn handler(client: Client, update: Update, session: Arc<SqliteSession>) {
    match update {
        Update::NewMessage(message) => {
            let peer_id = message.peer_id();
            let peer_ref = message
                .peer_ref()
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| peer_id.to_ambient_ref());
            if let Some(action) = message.action() {
                use tl::enums::MessageAction as A;
                if match action {
                    A::ChatJoinedByLink(_) => true,
                    A::ChatAddUser(_) => true,
                    A::ChatDeleteUser(_) => true,
                    A::ChatJoinedByRequest => true,
                    A::ChatJoinedViaCommunity(_) => true,
                    _ => false,
                } {
                    let _ = message.delete().await;
                }
                return;
            }
            if message.outgoing() {
                return;
            }

            if let Some(guestchat_via_from_id) = message.guestchat_via_from_id() {
                let guestchat_via_from_ref = message
                    .guestchat_via_from_ref()
                    .await
                    .unwrap_or_default()
                    .unwrap_or_else(|| guestchat_via_from_id.to_ambient_ref());
                match client
                    .set_banned_rights(peer_ref, guestchat_via_from_ref)
                    .view_messages(false)
                    .await
                {
                    Ok(_) => {
                        log::info!("封禁 guest 触发者 {guestchat_via_from_id} 成功");
                    }
                    Err(e) => {
                        log::warn!("封禁 guest 触发者 {guestchat_via_from_id} 失败: {e}");
                    }
                }

                if let Ok(Some(reply)) = message.get_reply().await {
                    let _ = reply.delete().await;
                }

                let sender_id = message.sender_id().unwrap();
                let sender_ref = message
                    .sender_ref()
                    .await
                    .unwrap_or_default()
                    .unwrap_or_else(|| sender_id.to_ambient_ref());
                match client
                    .set_banned_rights(peer_ref, sender_ref)
                    .view_messages(false)
                    .await
                {
                    Ok(_) => {
                        log::info!("封禁 guestbot {sender_id} 成功");
                    }
                    Err(e) => {
                        log::warn!("封禁 guestbot {sender_id} 失败: {e}");
                    }
                }

                let _ = message.delete().await;
            }
        }
        Update::CallbackQuery(callback) => {
            let data = callback.data();
            if let Some((verify_user_id, index)) = VerifyButton::from_data(&data) {
                handle_verify(client, callback, verify_user_id, index).await;
            } else if let Some(user_id) = AdminVerifyButton::from_data(&data) {
                handle_admin_verify(client, callback, user_id).await;
            } else if let Some(user_id) = AdminKickButton::from_data(&data) {
                handle_admin_kick(client, callback, user_id).await;
            }
        }
        /*#[cfg(debug_assertions)]
        Update::GuestChatQuery(query) => {
            let _ = query
                .answer(grammers_client::update::Article::new("标题", "测试1"))
                .await;
        }*/
        Update::Raw(raw) => match raw.raw {
            tl::enums::Update::ChatParticipantAdd(update) => {
                let chat = PeerId::chat(update.chat_id).unwrap().to_ambient_ref();
                let user_id = PeerId::user(update.user_id).unwrap();
                let user = session
                    .peer_ref(user_id)
                    .await
                    .unwrap_or_default()
                    .unwrap_or_else(|| user_id.to_ambient_ref());
                send_verify(client, chat, user).await;
            }
            tl::enums::Update::ChannelParticipant(update) => {
                if let Some(new_participant) = update.new_participant {
                    match new_participant {
                        tl::enums::ChannelParticipant::Participant(_) => {
                            let channel =
                                PeerId::channel(update.channel_id).unwrap().to_ambient_ref();
                            if let Ok(peer) = client.resolve_peer(channel).await {
                                let peer_ref =
                                    peer.to_ref().await.unwrap_or_default().unwrap_or(channel);
                                let user_id = PeerId::user(update.user_id).unwrap();
                                let user = session
                                    .peer_ref(user_id)
                                    .await
                                    .unwrap_or_default()
                                    .unwrap_or_else(|| user_id.to_ambient_ref());
                                send_verify(client, peer_ref, user).await;
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

async fn send_verify(client: Client, peer_ref: PeerRef, user_ref: PeerRef) {
    let peer_id = peer_ref.id;
    let user_id = user_ref.id;
    if !CONFIG.is_enabled(peer_id).await {
        return;
    }

    let status = match get_status(peer_id, user_id).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("获取验证状态失败: {e}");
            return;
        }
    };
    match status {
        VerifyStatus::Verified => {
            return;
        }
        VerifyStatus::Verifying { message_id, .. } => {
            if let Err(e) = client.delete_messages(peer_ref, &[message_id]).await {
                log::error!("删除消息失败: {e}");
            }
        }
        VerifyStatus::Null | VerifyStatus::Banned => {
            match client
                .set_banned_rights(peer_ref, user_ref)
                .send_messages(false)
                .send_media(false)
                .send_stickers(false)
                .send_gifs(false)
                .send_games(false)
                .send_inline(false)
                .embed_link_previews(false)
                .send_polls(false)
                .await
            {
                Ok(_) => {
                    log::info!("禁言入群者 {user_id} 成功");
                }
                Err(e) => {
                    log::error!("禁言入群者 {user_id} 失败: {e}");
                    return;
                }
            }
        }
    }

    let now = Timestamp::now();
    let user = client.resolve_peer(user_ref).await.unwrap();
    let user_name = user.name().unwrap_or("Unknown");
    let user_name = truncate_name(user_name);
    let user_url = if let Some(username) = user.username() {
        format!("https://t.me/{username}")
    } else {
        format!("tg://user?id={}", user.id())
    };

    let (emoji_indexs, solution, solution_emoji) = {
        let mut rng = rng();
        let emoji_indexs: Vec<u32> = (0..(EMOJIS.len() as u32)).sample(&mut rng, 9);
        // 保证第一个按钮 (index 0) 不会是正确答案
        let rand_index: usize = random_range(1..9);
        let solution: u32 = emoji_indexs[rand_index];
        let solution_emoji = EMOJIS[solution as usize];
        (emoji_indexs, solution, solution_emoji)
    };
    let mut buttons: Vec<Vec<Button>> = emoji_indexs
        .chunks(3)
        .map(|indexs| {
            indexs
                .into_iter()
                .map(|index| VerifyButton::new(user_id, *index))
                .collect()
        })
        .collect();
    buttons.push(vec![
        AdminVerifyButton::new(user_id),
        AdminKickButton::new(user_id),
    ]);
    let text = format!(
        "<a href=\"{user_url}\">{user_name}</a> 请在 {} 秒内点击 {} 完成入群验证",
        VERIFY_LIMIT.as_secs(),
        solution_emoji
    );
    let reply_markup = ReplyMarkup::from_buttons(&buttons);
    let message = match client
        .send_message(
            peer_ref,
            InputMessage::new()
                .html(text)
                .link_preview(false)
                .reply_markup(reply_markup),
        )
        .await
    {
        Ok(m) => m,
        Err(e) => {
            log::error!("发送消息失败: {e}");
            return;
        }
    };

    log::info!("solution: {solution}");
    if let Err(e) = set_status(
        peer_id,
        user_id,
        VerifyStatus::Verifying {
            date: now,
            message_id: message.id(),
            solution,
        },
    )
    .await
    {
        log::error!("设置验证状态失败: {e}");
    }
}

fn truncate_name(s: &str) -> String {
    let mut result = String::with_capacity(10);
    let mut count = 0;
    let mut truncated = false;

    for c in s.chars() {
        if c.is_ascii() {
            if count >= 7 {
                truncated = true;
                break;
            }
            result.push(c);
            count += 1;
        } else {
            if count < 2 {
                result.push(c);
                count += 1;
            } else {
                truncated = true;
                break;
            }
        }
    }

    if truncated {
        result.push_str("...");
    }
    result
}

pub struct VerifyButton;

impl VerifyButton {
    const ID: [u8; 4] = crate::id!("verify");

    pub fn new(user_id: PeerId, index: u32) -> Button {
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&Self::ID);
        let user_id = user_id.bot_api_dialog_id().unwrap();
        data.extend_from_slice(&user_id.to_le_bytes());
        data.extend_from_slice(&index.to_le_bytes());
        Button::data(EMOJIS[index as usize], data)
    }

    pub fn from_data(data: &[u8]) -> Option<(PeerId, u32)> {
        if &data[..4] == &Self::ID {
            let user_id = i64::from_le_bytes(data[4..12].try_into().unwrap());
            let user_id = PeerId::from_bot_api_dialog_id(user_id).unwrap();
            let index = u32::from_le_bytes(data[12..16].try_into().unwrap());
            Some((user_id, index))
        } else {
            None
        }
    }
}

async fn handle_verify(
    client: Client,
    callback: CallbackQuery,
    verify_user_id: PeerId,
    index: u32,
) {
    let user_id = callback.sender_id();
    if user_id != verify_user_id {
        if let Err(e) = callback.answer().alert("这是别人的入群验证啦").send().await {
            log::error!("alert失败: {e}");
        }
        return;
    }

    log::info!("handle_verify: {index}");
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| peer_id.to_ambient_ref());
    let user_ref = callback
        .sender_ref()
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| user_id.to_ambient_ref());
    let msg_id = match &callback.raw {
        tl::enums::Update::BotCallbackQuery(update) => Some(update.msg_id),
        _ => None,
    };

    let status = match get_status(peer_id, user_id).await {
        Ok(s) => s,
        Err(e) => {
            log::error!("获取验证状态失败: {e}");
            return;
        }
    };
    log::info!("{status:?}");
    match status {
        VerifyStatus::Null => {}
        VerifyStatus::Verifying { date, solution, .. } => {
            let now = Timestamp::now();
            if now.duration_since(date) < VERIFY_LIMIT && solution == index {
                if let Err(e) = set_status(peer_id, user_id, VerifyStatus::Verified).await {
                    log::error!("设置验证状态失败: {e}");
                }

                match client
                    .set_banned_rights(peer_ref, user_ref)
                    .send_messages(true)
                    .send_media(true)
                    .send_stickers(true)
                    .send_gifs(true)
                    .send_games(true)
                    .send_inline(true)
                    .embed_link_previews(true)
                    .send_polls(true)
                    .await
                {
                    Ok(_) => {
                        log::info!("解除验证通过者禁言 {user_id} 成功");
                    }
                    Err(e) => {
                        log::error!("解除验证通过者禁言 {user_id} 失败: {e}");
                    }
                }
            } else {
                if let Err(e) = set_status(peer_id, user_id, VerifyStatus::Banned).await {
                    log::error!("设置验证状态失败: {e}");
                }

                match client
                    .set_banned_rights(peer_ref, user_ref)
                    .view_messages(false)
                    .await
                {
                    Ok(_) => {
                        log::info!("封禁验证失败者 {user_id} 成功");
                    }
                    Err(e) => {
                        log::warn!("封禁验证失败者 {user_id} 失败: {e}");
                    }
                }
            }
        }
        VerifyStatus::Verified => {}
        VerifyStatus::Banned => {}
    }

    // let _ = callback.answer().send().await;
    if let Some(msg_id) = msg_id {
        let _ = client.delete_messages(peer_ref, &[msg_id]).await;
    }
}

#[crate::on_interval]
async fn on_interval(client: Client, session: Arc<SqliteSession>) {
    let all_verifying = match database::get_all_verifying().await {
        Ok(x) => x,
        Err(e) => {
            log::error!("获取正在验证列表失败: {e}");
            return;
        }
    };
    for (peer_id, user_id, status) in all_verifying.into_iter() {
        let VerifyStatus::Verifying {
            date, message_id, ..
        } = status
        else {
            continue;
        };

        let now = Timestamp::now();
        if now.duration_since(date) > VERIFY_LIMIT {
            let peer_ref = session
                .peer_ref(peer_id)
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| peer_id.to_ambient_ref());
            let user_ref = session
                .peer_ref(user_id)
                .await
                .unwrap_or_default()
                .unwrap_or_else(|| user_id.to_ambient_ref());
            let _ = client.delete_messages(peer_ref, &[message_id]).await;

            if let Err(e) = set_status(peer_id, user_id, VerifyStatus::Banned).await {
                log::error!("设置验证状态失败: {e}");
            }

            match client
                .set_banned_rights(peer_ref, user_ref)
                .view_messages(false)
                .await
            {
                Ok(_) => {
                    log::info!("封禁验证超时者 {user_id} 成功");
                }
                Err(e) => {
                    log::warn!("封禁验证超时者 {user_id} 失败: {e}");
                }
            }
        }
    }
}

pub struct AdminVerifyButton;

impl AdminVerifyButton {
    const ID: [u8; 4] = crate::id!("verify_admin");

    pub fn new(user_id: PeerId) -> Button {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&Self::ID);
        let user_id = user_id.bot_api_dialog_id().unwrap();
        data.extend_from_slice(&user_id.to_le_bytes());
        Button::data("放行 (管理员)", data)
    }

    pub fn from_data(data: &[u8]) -> Option<PeerId> {
        if &data[..4] == &Self::ID {
            let user_id = i64::from_le_bytes(data[4..12].try_into().unwrap());
            let user_id = PeerId::from_bot_api_dialog_id(user_id).unwrap();
            Some(user_id)
        } else {
            None
        }
    }
}

async fn handle_admin_verify(client: Client, callback: CallbackQuery, user_id: PeerId) {
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| peer_id.to_ambient_ref());
    let admin_id = callback.sender_id();
    let admin_ref = callback
        .sender_ref()
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| admin_id.to_ambient_ref());
    let permissions = match client.get_permissions(peer_ref, admin_ref).await {
        Ok(x) => x,
        Err(e) => {
            log::error!("获取用户权限失败: {e}");
            return;
        }
    };
    if !permissions.is_admin() {
        if let Err(e) = callback.answer().alert("没有管理员权限").send().await {
            log::error!("alert失败: {e}");
        }
        return;
    }

    let msg_id = match &callback.raw {
        tl::enums::Update::BotCallbackQuery(update) => Some(update.msg_id),
        _ => None,
    };

    if let Err(e) = set_status(peer_id, user_id, VerifyStatus::Verified).await {
        log::error!("设置验证状态失败: {e}");
        let _ = callback
            .answer()
            .alert("放行失败: 设置状态错误")
            .send()
            .await;
        return;
    }

    let user_ref = user_id.to_ambient_ref();
    let user_ref = match client.resolve_peer(user_ref).await {
        Err(_) => user_ref,
        Ok(user) => user.to_ref().await.unwrap_or_default().unwrap_or(user_ref),
    };
    let success = match client
        .set_banned_rights(peer_ref, user_ref)
        .send_messages(true)
        .send_media(true)
        .send_stickers(true)
        .send_gifs(true)
        .send_games(true)
        .send_inline(true)
        .embed_link_previews(true)
        .send_polls(true)
        .await
    {
        Ok(_) => {
            log::info!("解除验证通过者禁言 {user_id} 成功");
            true
        }
        Err(e) => {
            log::error!("解除验证通过者禁言 {user_id} 失败: {e}");
            false
        }
    };

    if success {
        let _ = callback.answer().alert("放行成功").send().await;
        if let Some(msg_id) = msg_id {
            let _ = client.delete_messages(peer_ref, &[msg_id]).await;
        }
    } else {
        let _ = callback.answer().alert("放行失败").send().await;
    }
}

pub struct AdminKickButton;

impl AdminKickButton {
    const ID: [u8; 4] = crate::id!("verify_admin_kick");

    pub fn new(user_id: PeerId) -> Button {
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&Self::ID);
        let user_id = user_id.bot_api_dialog_id().unwrap();
        data.extend_from_slice(&user_id.to_le_bytes());
        Button::data("封禁 (管理员)", data)
    }

    pub fn from_data(data: &[u8]) -> Option<PeerId> {
        if &data[..4] == &Self::ID {
            let user_id = i64::from_le_bytes(data[4..12].try_into().unwrap());
            let user_id = PeerId::from_bot_api_dialog_id(user_id).unwrap();
            Some(user_id)
        } else {
            None
        }
    }
}

async fn handle_admin_kick(client: Client, callback: CallbackQuery, user_id: PeerId) {
    let peer_id = callback.peer_id();
    let peer_ref = callback
        .peer_ref()
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| peer_id.to_ambient_ref());
    let admin_id = callback.sender_id();
    let admin_ref = callback
        .sender_ref()
        .await
        .unwrap_or_default()
        .unwrap_or_else(|| admin_id.to_ambient_ref());
    let permissions = match client.get_permissions(peer_ref, admin_ref).await {
        Ok(x) => x,
        Err(e) => {
            log::error!("获取用户权限失败: {e}");
            return;
        }
    };
    if !permissions.is_admin() {
        if let Err(e) = callback.answer().alert("没有管理员权限").send().await {
            log::error!("alert失败: {e}");
        }
        return;
    }

    let msg_id = match &callback.raw {
        tl::enums::Update::BotCallbackQuery(update) => Some(update.msg_id),
        _ => None,
    };

    if let Err(e) = set_status(peer_id, user_id, VerifyStatus::Banned).await {
        log::error!("设置验证状态失败: {e}");
        let _ = callback
            .answer()
            .alert("封禁失败: 设置状态错误")
            .send()
            .await;
        return;
    }

    let user_ref = user_id.to_ambient_ref();
    let user_ref = match client.resolve_peer(user_ref).await {
        Err(_) => user_ref,
        Ok(user) => user.to_ref().await.unwrap_or_default().unwrap_or(user_ref),
    };
    let success = match client
        .set_banned_rights(peer_ref, user_ref)
        .view_messages(false)
        .await
    {
        Ok(_) => {
            log::info!("封禁验证失败者 {user_id} 成功");
            true
        }
        Err(e) => {
            log::warn!("封禁验证失败者 {user_id} 失败: {e}");
            false
        }
    };

    if success {
        let _ = callback.answer().alert("封禁成功").send().await;
        if let Some(msg_id) = msg_id {
            let _ = client.delete_messages(peer_ref, &[msg_id]).await;
        }
    } else {
        let _ = callback.answer().alert("封禁失败").send().await;
    }
}
