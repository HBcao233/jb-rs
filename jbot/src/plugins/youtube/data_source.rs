use std::path::Path;

use jb_core::youtube::get_video_info;
use jb_core::youtube::types::{VideoDetails, YoutubeError};
use tokio::fs;
use wreq::Client;

pub(super) async fn get_info(
    client: &Client,
    video_id: &str,
) -> Result<VideoDetails, YoutubeError> {
    let cache_path = Path::new("cache/youtube/");
    if let Err(e) = fs::create_dir_all(cache_path).await {
        return Err(YoutubeError::Io(e));
    }

    let cache_file = cache_path.join(&format!("youtube_{video_id}.json"));
    get_video_info(client, video_id, cache_file).await
}

pub fn parse_msg(info: &VideoDetails) -> String {
    let video_id = &info.video_id;
    let title = &info.title;
    let author = &info.author;
    let channel_id = &info.channel_id;
    let mut desc = info.short_description.to_string();
    if !desc.is_empty() {
        desc = crate::utils::safe_truncate(&desc, 900);
        desc = format!("\n<blockquote expandable>{desc}</blockquote>");
    }
    format!(
        "<a href=\"https://www.youtube.com/watch?v={video_id}\">{title}</a> - \
         <a href=\"https://www.youtube.com/channel/{channel_id}\">{author}</a> #YouTuBe{desc}"
    )
}
