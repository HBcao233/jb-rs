use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use grammers_client::client::Client;
use grammers_client::message::{Button, InputMessage, Message, ReplyMarkup};
use grammers_session::types::{PeerId, PeerKind};
use tokio::time::{Duration, sleep};
use tracing::error;

const GROUPED_INTERVAL: Duration = Duration::from_millis(500);

// peer_id: GroupedMedias
type PeerGroupedMedias = BTreeMap<i64, GroupedMedias>;
static PEER_GROUPED_MEDIAS: OnceLock<Mutex<PeerGroupedMedias>> = OnceLock::new();

fn peer_grouped_medias() -> &'static Mutex<PeerGroupedMedias> {
    PEER_GROUPED_MEDIAS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

async fn grouped_messages_handler(client: Client, messages: Vec<Arc<Message>>) {
    let message = messages.first().unwrap();
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
        .unwrap_or_default()
        .unwrap_or_else(|| peer_id.to_ambient_ref());

    let text = format!("收到 {} 条媒体", messages.len());
    let message_ids: Vec<i32> = messages.iter().map(|m| m.id()).collect();
    #[allow(unused_variables)]
    let media = message.media().unwrap();

    #[allow(unused_mut)]
    let mut buttons: Vec<Vec<Button>> = Vec::new();

    #[cfg(feature = "spoiler")]
    {
        use grammers_client::media::Media;

        let spoilered = match media {
            Media::Photo(p) => p.is_spoiler(),
            Media::Document(d) => d.is_spoiler(),
            _ => false,
        };
        buttons.push(vec![crate::plugins::spoiler::SwitchSpoilerButton::new(
            false,
            spoilered,
            &message_ids,
        )]);
    }

    #[cfg(feature = "merge")]
    buttons.push(vec![
        crate::plugins::merge::AddMergeButton::new(&message_ids),
        crate::plugins::merge::DirectMergeButton::new(&message_ids),
    ]);

    if buttons.is_empty() {
        return;
    }

    let reply_markup = ReplyMarkup::from_buttons(&buttons);
    if let Err(e) = client
        .send_message(
            peer_ref,
            InputMessage::new()
                .text(text)
                .reply_to(message_ids.first().copied())
                .reply_markup(reply_markup),
        )
        .await
    {
        error!("合并button发送失败: {e}");
    }
}

struct GroupedMedias {
    client: Client,
    ended: Arc<AtomicBool>,
    task: Option<tokio::task::JoinHandle<()>>,
    messages: Vec<Arc<Message>>,
    peer_id: i64,
}

impl GroupedMedias {
    pub fn new(client: Client, message: Arc<Message>, peer_id: i64) -> Self {
        let mut messages = Vec::with_capacity(10);
        messages.push(message);
        let mut group = Self {
            client,
            ended: Arc::new(AtomicBool::new(false)),
            task: None,
            messages,
            peer_id,
        };
        group.schedule_task();
        group
    }

    pub fn push(&mut self, message: Arc<Message>) {
        if self.ended.load(Ordering::SeqCst) {
            error!("GroupedMedias 在任务执行完后再次被 push。");
            return;
        }

        self.messages.push(message);
        if let Some(task) = &self.task {
            task.abort();
        }
        if self.messages.len() >= 10 {
            self.ended.store(true, Ordering::SeqCst);
        }

        self.schedule_task();
    }

    fn schedule_task(&mut self) {
        let ended = Arc::clone(&self.ended);
        let client = self.client.clone();
        let messages = self.messages.clone();
        let peer_id = self.peer_id.clone();

        self.task = Some(tokio::spawn(async move {
            if messages.len() < 10 {
                sleep(GROUPED_INTERVAL).await;
            }
            if ended.load(Ordering::SeqCst) {
                return;
            }
            ended.store(true, Ordering::SeqCst);

            grouped_messages_handler(client.clone(), messages.clone()).await;

            let mut guard = peer_grouped_medias().lock().unwrap();
            if let Some(current_group) = guard.get(&peer_id) {
                if Arc::ptr_eq(&current_group.ended, &ended) {
                    guard.remove(&peer_id);
                }
            }
        }));
    }
}

pub(crate) fn get_or_insert(client: Client, peer_id: PeerId, message: Arc<Message>) {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let mut guard = peer_grouped_medias().lock().unwrap();

    if let Some(group) = guard.get_mut(&peer_id) {
        if !group.ended.load(Ordering::SeqCst) {
            group.push(message);
            return;
        }
    }

    let group = GroupedMedias::new(client, message, peer_id);
    guard.insert(peer_id, group);
}
