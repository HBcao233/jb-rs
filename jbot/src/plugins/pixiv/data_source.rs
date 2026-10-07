use jb_core::pixiv::fetch_info;
use jb_core::pixiv::types::{PixivDetails, PixivError};
use regex::regex;
use tokio::fs;
use tracing::error;
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
    fetch_info(client, pid, cookies, &cache_file).await
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
