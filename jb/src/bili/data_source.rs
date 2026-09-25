use std::path::Path;

use wreq::Client;

pub use jb_core::bili::parse_desc;
use jb_core::bili::types::{BiliInfo, GetBiliError, GetPlayurlError, PlayurlInfo};
use jb_core::bili::{fetch_bili_info, fetch_playurl};

pub async fn get_bili_info(
    client: &Client,
    aid: u64,
    bvid: &str,
) -> Result<BiliInfo, GetBiliError> {
    fetch_bili_info(client, aid, bvid, Path::new(".")).await
}

pub async fn get_playurl(
    client: &Client,
    aid: u64,
    bvid: &str,
    cid: i64,
) -> Result<PlayurlInfo, GetPlayurlError> {
    fetch_playurl(client, aid, bvid, cid, Path::new(".")).await
}
