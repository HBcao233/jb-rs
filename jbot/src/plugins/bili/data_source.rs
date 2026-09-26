use std::path::Path;

use tokio::fs;
use tracing::error;
use wreq::Client;

use jb_core::bili::types::{BiliError, BiliInfo, Page, PlayurlInfo};
use jb_core::bili::{fetch_bili_info, fetch_playurl, parse_desc};

pub async fn get_bili(
    client: &Client,
    aid: u64,
    bvid: &str,
    grisk_id: Option<String>,
) -> Result<BiliInfo, BiliError> {
    let cache_dir = Path::new("cache/bilis");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        error!("缓存文件夹创建失败: {e:?}");
        return Err(BiliError::Io(e));
    }

    let mut cookies = Vec::new();
    if let Some(s) = super::SESSDATA.get().unwrap() {
        cookies.push(("SESSDATA", s.to_string()));
    }

    fetch_bili_info(client, aid, bvid, grisk_id, &cookies, &cache_dir).await
}

pub fn parse_msg(info: BiliInfo, p: u16) -> Result<(String, Page, String), ()> {
    let bvid = info.bvid;
    let p_url = if p > 1 {
        format!("?p={p}")
    } else {
        String::new()
    };
    let p_tip = if p > 1 {
        format!(" P{p}")
    } else {
        String::new()
    };

    let mut pa = None;
    for page in info.pages {
        if page.page == p {
            pa = Some(page);
            break;
        }
    }
    let pa = pa.ok_or(())?;

    let title = info
        .title
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let uid = info.owner.mid;
    let nickname = info.owner.name;
    let mut desc = parse_desc(&info.desc_v2.unwrap_or_default());
    if &desc == "-" || &desc == "." || &desc == "。" {
        desc.clear();
    }
    if !desc.is_empty() {
        desc = crate::utils::safe_truncate(&desc, 900);
        desc = format!(":\n<blockquote expandable>{desc}</blockquote>");
    }

    let msg = format!(
        "<a href=\"https://www.bilibili.com/video/{bvid}{p_url}\">{title}{p_tip}</a> | \
         <a href=\"https://space.bilibili.com/{uid}\">{nickname}</a> #Bilibili{desc}"
    );
    Ok((msg, pa, info.pic))
}

pub async fn get_playurl(
    client: &Client,
    aid: u64,
    bvid: &str,
    cid: i64,
    grisk_id: Option<String>,
) -> Result<PlayurlInfo, BiliError> {
    let cache_dir = Path::new("cache/bilis");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        error!("缓存文件夹创建失败: {e:?}");
    }

    let mut cookies = Vec::new();
    if let Some(s) = super::SESSDATA.get().unwrap() {
        cookies.push(("SESSDATA", s.to_string()));
    }

    fetch_playurl(client, aid, bvid, cid, grisk_id, &cookies, &cache_dir).await
}
