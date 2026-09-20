use std::sync::Arc;

use grammers_client::Client;
use grammers_client::message::Message;
use grammers_client::update::Update;
use grammers_session::storages::SqliteSession;

use crate::database as db;

pub type HandlerResult = std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>;
type Handler = fn(client: Client, update: Update, session: Arc<SqliteSession>) -> HandlerResult;
type NewMessageHandler = fn(client: Client, message: Arc<Message>) -> HandlerResult;
type GroupedMessagesHandler = fn(client: Client, message: Vec<Arc<Message>>) -> HandlerResult;

#[linkme::distributed_slice]
pub static SETUPS: [fn() -> anyhow::Result<()>];

#[linkme::distributed_slice]
pub static INTERVAL_HANDLERS: [fn(client: Client, session: Arc<SqliteSession>) -> HandlerResult];

#[linkme::distributed_slice]
pub static HANDLERS: [Handler];

#[linkme::distributed_slice]
pub static NEW_MESSAGE_HANDLERS: [NewMessageHandler];

#[linkme::distributed_slice]
pub static GROUPED_MESSAGES_HANDLERS: [GroupedMessagesHandler];

pub(crate) async fn handle_update(client: Client, update: Update, session: Arc<SqliteSession>) {
    for handler in HANDLERS {
        handler(client.clone(), update.clone(), Arc::clone(&session)).await;
    }

    match update {
        Update::NewMessage(message) => {
            let peer_id = message.peer_id();
            if !message.outgoing() && message.action().is_none() {
                if let Some(sender_id) = message.sender_id() {
                    let sender_info = crate::utils::get_peer_info(&sender_id, message.sender());
                    let text = crate::utils::safe_truncate(message.text(), 30);

                    let peer_info = if sender_id != peer_id {
                        let peer_info = crate::utils::get_peer_info(&peer_id, message.peer());
                        &format!(" in {peer_info}")
                    } else {
                        ""
                    };
                    log::info!("{sender_info}{}: {text}", peer_info);
                }
            }

            if let Err(e) = db::insert_from_message(&message, None).await {
                log::error!("添加缓存媒体失败: {e}");
            }

            let message = Arc::new(message.into_inner());
            for handler in NEW_MESSAGE_HANDLERS {
                handler(client.clone(), Arc::clone(&message)).await;
            }

            if let Some(media) = message.media() {
                if crate::utils::can_grouped(&media) {
                    super::grouped::get_or_insert(client.clone(), peer_id, message);
                }
            }
        }
        _ => {}
    }
}
