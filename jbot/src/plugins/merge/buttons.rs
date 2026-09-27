use grammers_client::message::Button;

pub struct AddMergeButton;

impl AddMergeButton {
    const ID: [u8; 4] = crate::id!("add_merge");

    pub fn new(message_ids: &[i32]) -> Button {
        if message_ids.len() > 10 {
            panic!("message_ids 长度不能大于 10");
        }

        let mut data = Vec::with_capacity(44);
        data.extend_from_slice(&Self::ID);
        for &num in message_ids {
            data.extend_from_slice(&num.to_le_bytes());
        }
        Button::data("合并媒体", data)
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

pub struct DirectMergeButton;

impl DirectMergeButton {
    const ID: [u8; 4] = crate::id!("direct_merge");

    pub fn new(message_ids: &[i32]) -> Button {
        if message_ids.len() > 10 {
            panic!("message_ids 长度不能大于 10");
        }

        let mut data = Vec::with_capacity(44);
        data.extend_from_slice(&Self::ID);
        for &num in message_ids {
            data.extend_from_slice(&num.to_le_bytes());
        }
        Button::data("直接合并", data)
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

pub struct FinishMergeButton;

impl FinishMergeButton {
    const ID: [u8; 4] = crate::id!("finish_merge");

    pub fn new() -> Button {
        Button::data("完成合并", Self::ID)
    }

    pub fn from_data(data: &[u8]) -> Option<()> {
        if &data[..4] == Self::ID {
            Some(())
        } else {
            None
        }
    }
}
