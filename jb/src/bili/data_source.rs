use std::path::Path;

use wreq::Client;

pub use jb_core::bili::parse_desc;
use jb_core::bili::types::{BiliError, BiliInfo, PlayurlInfo};
use jb_core::bili::{fetch_bili_info, fetch_playurl};

pub async fn get_bili_info(client: &Client, aid: u64, bvid: &str) -> Result<BiliInfo, BiliError> {
    fetch_bili_info(client, aid, bvid, None, &mut vec![], Path::new(".")).await
}

pub async fn get_playurl(
    client: &Client,
    aid: u64,
    bvid: &str,
    cid: i64,
) -> Result<PlayurlInfo, BiliError> {
    fetch_playurl(client, aid, bvid, cid, None, &mut vec![], Path::new(".")).await
}
