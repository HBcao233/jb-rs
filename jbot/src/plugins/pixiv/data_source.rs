use std::path::PathBuf;
use std::process::ExitStatus;

use jb_core::pixiv::types::{Frame, PixivDetails, PixivError, UgoiraMeta};
use jb_core::pixiv::{fetch_info, fetch_ugoira_meta};
use regex::regex;
use tokio::fs;
use tokio::io::{self, AsyncWriteExt};
use tokio::process::Command;
use tracing::{error, warn};
use wreq::Client;

const MAX_COMMENT_LENGTH: usize = 600;

pub(super) async fn get_info(client: &Client, pid: &str) -> Result<PixivDetails, PixivError> {
    let cache_dir = crate::cache_dir().join("pixiv");
    if let Err(e) = fs::create_dir_all(&cache_dir).await {
        error!("缓存文件夹创建失败: {e:?}");
        return Err(PixivError::Io(e));
    }

    let mut cookies = Vec::new();
    if let Some(s) = super::PHPSESSID.get().unwrap() {
        cookies.push(("PHPSESSID", s.to_string()));
    }

    let name = format!("{pid}.json");
    let cache_file = cache_dir.join(name);
    let res = fetch_info(client, pid, cookies, &cache_file).await?;
    if let Some(ref mask_reason) = res.illust_details.mask_reason {
        warn!("媒体获取失败: {mask_reason}");
        fs::remove_file(cache_file).await?;
    }
    Ok(res)
}

pub fn parse_msg(info: &PixivDetails) -> String {
    let mut props = Vec::new();
    if info.illust_details.ai_type == 2 {
        props.push("#AI生成");
    }
    let mut r18 = false;
    let mut r17_9 = false;
    let mut r18g = false;
    for tag in info.illust_details.tags.iter() {
        match tag.as_str() {
            "R-18" => r18 = true,
            "R-17.9" => r17_9 = true,
            "R-18G" => r18g = true,
            _ => {}
        }
    }
    if r18 || r17_9 || r18g {
        props.push("#NSFW");
    }
    if r18 {
        props.push("#R18");
    }
    if r17_9 {
        props.push("#R17.9");
    }
    if r18g {
        props.push("#R18G");
    }
    if info.illust_details.r#type == "2" {
        props.push("#动图");
    }

    let props = if props.len() > 0 {
        format!("{}\n", props.join(" "))
    } else {
        String::new()
    };

    let pid = &info.illust_details.id;
    let title = &info.illust_details.title;
    let uid = &info.illust_details.user_id;
    let username = &info.author_details.user_account;

    let comment = info
        .illust_details
        .comment_html
        .clone()
        .unwrap_or_default()
        .replace("<br />", "\n")
        .replace("<br/>", "\n")
        .replace("<br>", "\n")
        .replace(" target=\"_blank\"", "");

    let re = regex!(r"<span[^>]*>(.*?)</span>");
    let comment = re.replace_all(&comment, "$1");

    let mut comment = truncate_comment(&comment, MAX_COMMENT_LENGTH);
    if !comment.is_empty() {
        comment = format!(":\n<blockquote expandable>{comment}</blockquote>");
    }

    format!(
        "{props}<a href=\"https://www.pixiv.net/artworks/{pid}/\">{title}</a> | \
        <a href=\"https://www.pixiv.net/users/{uid}/\">{username}</a> #pixiv [<code>{pid}</code>]{comment}",
    )
}

fn truncate_comment(comment: &str, max_comment_length: usize) -> String {
    if comment.chars().count() <= max_comment_length {
        return comment.to_string();
    }

    let mut comment: String = comment.chars().take(max_comment_length).collect();

    let re = regex!(r"<[^/]+[^<]*(<[^>]*)?$");
    comment = re.replace(&comment, "").into_owned();

    if comment.ends_with('\n') {
        comment.pop();
    }

    comment.push_str("\n......");
    comment
}

pub async fn get_ugoira_meta(client: &Client, pid: &str) -> Result<UgoiraMeta, PixivError> {
    let cache_dir = crate::cache_dir().join("pixiv");
    if let Err(e) = fs::create_dir_all(&cache_dir).await {
        error!("缓存文件夹创建失败: {e:?}");
        return Err(PixivError::Io(e));
    }

    let mut cookies = Vec::new();
    if let Some(s) = super::PHPSESSID.get().unwrap() {
        cookies.push(("PHPSESSID", s.to_string()));
    }

    let name = format!("{pid}_ugoira_meta.json");
    let cache_file = cache_dir.join(name);
    fetch_ugoira_meta(client, pid, cookies, &cache_file).await
}

pub async fn unzip_ugoira(input: &PathBuf, output: &PathBuf) -> io::Result<ExitStatus> {
    let ext = input.extension();
    let mut child = match ext.and_then(|s| s.to_str()) {
        Some("zip") => Command::new("unzip")
            .arg("-d")
            .arg(output)
            .arg(input)
            .kill_on_drop(true)
            .spawn()?,
        Some("tar") | Some("gz") => Command::new("unzip")
            .arg("xf")
            .arg("-C")
            .arg(output)
            .arg(input)
            .kill_on_drop(true)
            .spawn()?,
        _ => {
            return Err(io::Error::other("暂不支持解压的类型"));
        }
    };
    child.wait().await
}

pub async fn create_ugoira_frames_txt(
    frames: &[Frame],
    ugoira_dir: &PathBuf,
    frames_txt: &PathBuf,
) -> Result<u32, io::Error> {
    let mut duration = 0;
    let mut output = fs::File::create(frames_txt).await?;
    for frame in frames {
        let file = ugoira_dir.join(&frame.file);
        let abs_path = fs::canonicalize(&file).await?;
        duration += frame.delay;
        let delay = frame.delay as f64 / 1000.0;
        output
            .write_all(format!("file '{}'\n", abs_path.to_string_lossy()).as_bytes())
            .await?;
        output
            .write_all(format!("duration {delay:.3}\n").as_bytes())
            .await?;
    }
    output.flush().await?;
    Ok(duration)
}
