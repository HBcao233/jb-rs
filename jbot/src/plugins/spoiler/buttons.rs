use grammers_client::message::Button;
use tracing::warn;

pub struct SwitchSpoilerButton;

impl SwitchSpoilerButton {
    const ID: [u8; 4] = crate::id!("switch_spoiler");

    pub fn new(outgoing: bool, spoilered: bool, message_ids: &[i32]) -> Button {
        if message_ids.len() > 10 {
            panic!("message_ids 长度不能大于 10");
        }

        let mut data = Vec::with_capacity(45);
        data.extend_from_slice(&Self::ID);
        let flag = if outgoing { 1 } else { 0 };
        data.push(flag);
        for num in message_ids.iter() {
            data.extend_from_slice(&num.to_le_bytes());
        }

        let name = if spoilered {
            super::REMOVE_SPOILER
        } else {
            super::ADD_SPOILER
        };
        Button::data(name, data)
    }

    pub fn from_data(data: &[u8]) -> Option<(bool, Vec<i32>)> {
        if &data[..4] == Self::ID {
            let flag = data[4];
            let outgoing = (flag & 1) == 1;
            let mut message_ids: Vec<i32> = data[5..]
                .chunks_exact(4)
                .map(|chunk| i32::from_le_bytes(chunk.try_into().unwrap()))
                .collect();
            if message_ids.len() > 10 {
                warn!("message_ids 数量大于 10");
                message_ids.truncate(10);
            }
            Some((outgoing, message_ids))
        } else {
            None
        }
    }
}
