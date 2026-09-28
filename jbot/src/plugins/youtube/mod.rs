mod data_source;

use std::sync::Arc;

use grammers_client::Client;
use grammers_client::message::{InputMessage, Message};
use grammers_session::types::{PeerKind, PeerRef};
use regex::regex;
use tracing::{error, info};

use self::data_source::{get_info, parse_msg};

const HELP: &str = "Youtube 解析。用法: /youtube <url>";

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
        r"(?:(?:https?://)?(?:.\.)*(?:youtube\.com/(?:watch\?v=|shorts/)|youtu\.be/))([0-9a-zA-Z-_]{3,12})"
    );

    let msg_id = message.id();
    let text = message.text().to_string();
    // info!("text: {text}");

    let starts_with_command = text.starts_with("/youtube");

    let mut matched = false;
    if let Some(caps) = re.captures(&text) {
        let (_, [video_id]) = caps.extract();
        info!("video_id: {video_id}");

        matched = true;
        if let Err(e) = send_youtube(client.clone(), peer_ref, Some(msg_id), video_id).await {
            error!(e, "send_youtube failed");
        }
    }

    if !matched && starts_with_command {
        if let Err(e) = message.reply(HELP).await {
            error!("消息发送失败: {e}")
        }
    }
}

async fn send_youtube(
    client: Client,
    peer_ref: PeerRef,
    msg_id: Option<i32>,
    video_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let prefix = format!("[{video_id}]");
    let mid = client
        .send_message(
            peer_ref,
            InputMessage::new()
                .text(format!("{prefix} 请等待..."))
                .reply_to(msg_id),
        )
        .await?;

    let wreq_client = crate::core::curl::get_client().build()?;
    let info = match get_info(&wreq_client, video_id).await {
        Ok(info) => info,
        Err(e) => {
            error!("获取视频信息失败: {e}");
            mid.edit(format!("获取视频信息失败: {e}")).await?;
            return Ok(());
        }
    };

    let msg = parse_msg(&info);
    match client
        .send_message(peer_ref, InputMessage::new().html(msg).reply_to(msg_id))
        .await
    {
        Ok(_) => {}
        Err(e) => {
            error!("消息发送失败: {e}");
            mid.edit("消息发送失败").await?;
            return Ok(());
        }
    };

    mid.delete().await?;

    Ok(())
}
