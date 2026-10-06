use grammers_client::message::Button;

pub struct SoutuButton;

impl SoutuButton {
    const ID: [u8; 4] = crate::id!("soutu");

    pub fn new(message_id: i32) -> Button {
        let mut data = Vec::with_capacity(45);
        data.extend_from_slice(&Self::ID);
        data.extend_from_slice(&message_id.to_le_bytes());

        Button::data("搜图", data)
    }

    pub fn from_data(data: &[u8]) -> Option<i32> {
        if &data[..4] == Self::ID {
            let message_id = i32::from_le_bytes(data[4..8].try_into().unwrap());
            Some(message_id)
        } else {
            None
        }
    }
}
