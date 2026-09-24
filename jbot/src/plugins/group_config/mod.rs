use std::sync::Arc;

use grammers_client::Client;
use grammers_client::message::{Button, InputMessage, Message, ReplyMarkup};
use grammers_client::update::{CallbackQuery, Update};
use grammers_session::Session;
use grammers_session::storages::SqliteSession;
use grammers_session::types::{PeerId, PeerKind};
use tracing::{error, info};

use crate::database::{get_config, set_config};
use crate::id;

const ENABLED_VALUE: &str = "1";
const ENABLED_PREFIX: &str = "✅ ";

pub struct Config {
    pub key: &'static str,
    pub name: &'static str,
}

#[linkme::distributed_slice]
pub static CONFIGS: [Config];

impl Config {
    const ID: [u8; 4] = id!("group_config");

    pub async fn is_enabled(&self, peer_id: PeerId) -> bool {
        let value = get_config(peer_id, self.key).await.unwrap_or_default();
        value.as_deref() == Some(ENABLED_VALUE)
    }

    fn to_button(&self, peer_id: PeerId, enabled: bool) -> Button {
        let key = self.key.as_bytes();
        let mut data = Vec::with_capacity(key.len() + 12);
        data.extend_from_slice(&Self::ID);

        let peer_id: i64 = peer_id.bot_api_dialog_id().unwrap();
        data.extend_from_slice(&peer_id.to_le_bytes());
        data.extend_from_slice(key);

        let mut name = String::with_capacity(self.name.len() + ENABLED_PREFIX.len());
        if enabled {
            name.push_str(ENABLED_PREFIX);
        }
        name.push_str(self.name);
        Button::data(name, data)
    }

    fn from_data(data: &[u8]) -> Option<(PeerId, String)> {
        if &data[..4] == &Self::ID {
            let peer_id = i64::from_le_bytes(data[4..12].try_into().unwrap());
            let peer_id = PeerId::from_bot_api_dialog_id(peer_id).unwrap();
            let key = String::from_utf8(data[12..].to_vec()).unwrap();
            Some((peer_id, key))
        } else {
            None
        }
    }
}

#[crate::on_new_message]
async fn handler(client: Client, message: Arc<Message>) {
    if message.outgoing() {
        return;
    }
    let peer_id = message.peer_id();
    if peer_id.kind() != PeerKind::Channel {
        return;
    }

    let text = message.text();
    if text.starts_with("/config") {
        let peer = match message.peer() {
            Some(p) => p,
            None => &client.resolve_peer(peer_id.to_ambient_ref()).await.unwrap(),
        };

        let buttons = render_buttons(peer_id).await;

        if buttons.is_empty() {
            let _ = message.reply("暂无配置选项").await;
        } else {
            let text = format!("群聊 {} 配置:", peer.name().unwrap_or("Unknown"));
            if let Err(e) = message
                .reply(
                    InputMessage::new()
                        .text(text)
                        .reply_markup(ReplyMarkup::from_buttons_row(&buttons)),
                )
                .await
            {
                error!("消息发送失败: {e}")
            }
        }
    }
}

#[crate::on_update]
async fn callback_handler(client: Client, update: Update, session: Arc<SqliteSession>) {
    match update {
        Update::CallbackQuery(callback) => {
            let data = callback.data();
            if let Some((peer_id, key)) = Config::from_data(&data) {
                handle_config_button(client, callback, session, peer_id, &key).await;
            }
        }
        _ => {}
    }
}

async fn render_buttons(peer_id: PeerId) -> Vec<Button> {
    let mut buttons = Vec::with_capacity(CONFIGS.len());
    for config in CONFIGS.iter() {
        let enabled = config.is_enabled(peer_id).await;
        buttons.push(config.to_button(peer_id, enabled));
    }
    buttons
}

async fn handle_config_button(
    client: Client,
    callback: CallbackQuery,
    session: Arc<SqliteSession>,
    peer_id: PeerId,
    key: &str,
) {
    let peer_ref = session
        .peer_ref(peer_id)
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
            error!("获取用户权限失败: {e}");
            return;
        }
    };
    if !permissions.is_admin() {
        if let Err(e) = callback.answer().alert("没有管理员权限").send().await {
            error!("alert失败: {e}");
        }
        return;
    }

    let old = get_config(peer_id, &key).await.unwrap_or_default();
    info!("peer_id: {peer_id}, key: {key}, old: {old:?}");

    let new = if old.as_deref() == Some(ENABLED_VALUE) {
        ""
    } else {
        ENABLED_VALUE
    };
    if let Err(e) = set_config(peer_id, &key, new).await {
        error!("设置群聊配置失败: {e}");
    }

    let text = if new.is_empty() {
        "禁用成功"
    } else {
        "启用成功"
    };
    let buttons = render_buttons(peer_id).await;
    if let Err(e) = callback
        .answer()
        .alert(text)
        .edit(InputMessage::new().reply_markup(ReplyMarkup::from_buttons_row(&buttons)))
        .await
    {
        error!("回应按钮查询失败: {e}");
    }
}
