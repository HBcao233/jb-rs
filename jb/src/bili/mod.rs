mod data_source;

use std::process::exit;

use indicatif::BinaryBytes;
use jb_core::bili::types::{self, BiliId};
use regex::Regex;
use rustix::termios::tcgetwinsize;
use tracing::{error, info};

use self::data_source::{get_bili_info, get_playurl, parse_desc};
use crate::curl::{get_client, stream_download};
use crate::ffmpeg::FFmpeg;
use crate::{BLUE, CYAN, GREEN, NC, Options, RED, YELLOW, align_left, padding_left};

pub async fn crawler_bili(input: &str, options: &Options) {
    let mut text = input.to_string();
    let re = Regex::new(
        r"(?:(?:https?://)?bilibili\.com/video/)?(av\d{2,16}|(?:BV|bv)[0-9a-zA-Z]{8,12})",
    )
    .unwrap();
    let b23_re = Regex::new(r"(?:https?://)?b23\.tv\\?/([0-9a-zA-Z]{7,7})").unwrap();

    if let Some(caps) = b23_re.captures(&text) {
        let (_, [b23_id]) = caps.extract();
        info!("b23.tv: {b23_id}");
        let url = format!("https://b23.tv/{}", b23_id);
        let client = get_client().build().unwrap();
        let response = match client.get(url).send().await {
            Ok(r) => r,
            Err(e) => {
                error!("b23.tv请求失败: {e}");
                eprintln!("{RED}error{NC}: 短链解析失败");
                exit(400);
            }
        };
        if let Some(location) = response
            .headers()
            .get("location")
            .and_then(|l| l.to_str().ok())
        {
            text = location.to_string();
        } else {
            eprintln!("{RED}error{NC}: 短链解析失败");
            exit(400);
        }
    }

    if let Some(caps) = re.captures(&text) {
        let (_, [id]) = caps.extract();
        info!("input: {id}");
        let bili_id = if id.starts_with('a') {
            let id = id.strip_prefix("av").unwrap();
            BiliId::AV(id.parse().unwrap())
        } else {
            BiliId::BV(id.to_string())
        };

        let Ok((aid, bvid)) = bili_id.to_raw() else {
            eprintln!("{RED}error{NC}: 无效的 avid/bvid: {id}");
            exit(400);
        };
        if let Err(e) = parse_bili(aid, bvid, options).await {
            eprintln!("{RED}error{NC}: bili解析失败: {e}");
            exit(500);
        }
        exit(0);
    }
}

async fn parse_bili(
    aid: u64,
    bvid: String,
    options: &Options,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = get_client().build()?;
    let referer = format!("https://www.bilibili.com/video/{}/", bvid);
    let headers = vec![("referer", referer)];

    let types::BiliInfo {
        title,
        desc_v2,
        pic,
        pages,
        ..
    } = get_bili_info(&client, aid, &bvid, &options.cookies).await?;
    println!();
    println!(" {CYAN}{}{NC}  哔哩哔哩 Bilibili", align_left("Site:", 13));
    println!(" {CYAN}{}{NC}  {}", align_left("BVid:", 13), &bvid);
    println!(" {CYAN}{}{NC}  {}", align_left("Title:", 13), &title);
    let desc = match desc_v2 {
        Some(d) => parse_desc(&d),
        None => String::new(),
    };
    let desc = desc.trim();
    if desc.contains('\n') {
        println!(" {CYAN}Description:{NC}\n {}", padding_left(&desc, 1));
    } else {
        println!(" {CYAN}{}{NC}  {}", align_left("Description:", 13), desc);
    }

    let p = 1;
    let mut page = None;
    // if info.pages.len() > 2 {}
    for pa in pages {
        if pa.page == p {
            page = Some(pa);
            break;
        }
    }
    let types::Page { cid, .. } = page.unwrap_or_else(|| {
        eprintln!("{RED}error{NC}: 分集 {p} 不存在");
        exit(1)
    });

    let types::PlayurlInfo {
        quality,
        accept_quality,
        accept_description,
        dash,
        durl,
        ..
    } = get_playurl(&client, aid, &bvid, cid, &options.cookies).await?;

    let quality = quality.unwrap();
    let accept_quality = accept_quality.unwrap();
    let accept_description = accept_description.unwrap();

    let (videos, audios) = if let Some(dash) = dash {
        let types::DashInfo { video, audio } = dash;

        let videos = video
            .into_iter()
            .filter(|ref v| v.mime_type == "video/mp4")
            .map(|v| {
                let mut urls = Vec::with_capacity(1 + v.backup_url.len());
                urls.push(v.base_url);
                urls.extend(v.backup_url);
                Video {
                    id: v.id,
                    size: v.bandwidth,
                    urls,
                    codecs: Some(v.codecs),
                    codecid: Some(v.codecid),
                }
            })
            .collect();

        let audios = if let Some(audio) = audio {
            let audios: Vec<Audio> = audio
                .into_iter()
                .filter(|ref v| v.mime_type == "audio/mp4")
                .map(|v| {
                    let mut urls = Vec::with_capacity(1 + v.backup_url.len());
                    urls.push(v.base_url);
                    urls.extend(v.backup_url);
                    Audio {
                        id: v.id,
                        size: v.bandwidth,
                        urls,
                    }
                })
                .collect();
            Some(audios)
        } else {
            None
        };
        (videos, audios)
    } else if let Some(durl) = durl {
        let types::DurlInfo {
            url,
            backup_url,
            size,
        } = durl.into_iter().next().unwrap();
        let mut urls = Vec::with_capacity(1 + backup_url.len());
        urls.push(url);
        urls.extend(backup_url);
        let video = Video {
            id: quality,
            size,
            urls,
            codecs: None,
            codecid: None,
        };
        (vec![video], None)
    } else {
        eprintln!("{RED}error{NC}: 未解析到流信息");
        exit(500);
    };

    let size = tcgetwinsize(std::io::stdout()).unwrap();
    let width = size.ws_col as usize;

    let pad = if width >= 80 { "     " } else { "   " };
    if options.info {
        println!(" {CYAN}Videos:{NC}");

        let audio = audios.and_then(|audios| audios.into_iter().max_by_key(|x| x.id));
        for video in videos {
            let mut size = video.size as u64;
            if let Some(ref a) = audio {
                size += a.size as u64;
            }
            print_stream(
                pad,
                video.id,
                video.codecid,
                size,
                video.codecs,
                &accept_quality,
                &accept_description,
            );
        }

        exit(0);
    } else {
        println!(" {CYAN}Selected video:{NC}");
        let audio = audios.and_then(|audios| audios.into_iter().max_by_key(|x| x.id));
        let video = videos
            .into_iter()
            .max_by(|a, b| {
                // id 降序 codecid 升序
                a.id.cmp(&b.id).then_with(|| b.codecid.cmp(&a.codecid))
            })
            .unwrap();

        let mut size = video.size as u64;
        if let Some(ref a) = audio {
            size += a.size as u64;
        }

        print_stream(
            pad,
            video.id,
            video.codecid,
            size,
            video.codecs,
            &accept_quality,
            &accept_description,
        );

        let key = format!("{bvid}_{cid}");
        let name = format!("{key}.mp4");
        let video_name = format!(
            "{key}_video_{}{}.mp4",
            video.id,
            if let Some(c) = video.codecid {
                format!("-{c}")
            } else {
                String::new()
            }
        );

        let mut index = 0;
        loop {
            let url = match video.urls.get(index) {
                Some(url) => {
                    println!(
                        " {GREEN}Downloading{NC} {video_name}{}",
                        if index == 0 {
                            String::new()
                        } else {
                            format!("Retry {index}")
                        }
                    );
                    url
                }
                None => {
                    eprintln!("{RED}error{NC}: 所有视频下载尝试均失败");
                    exit(1);
                }
            };
            match stream_download(&client, url, &video_name, &headers).await {
                Ok(_) => {
                    break;
                }
                Err(e) => {
                    eprintln!("{YELLOW}warn{NC}: 下载失败: {e}");
                }
            }
            index += 1;
        }

        println!();
        if let Some(a) = audio {
            let audio_name = format!("{key}_audio_{}.mp4", a.id);
            println!(" {GREEN}Downloading{NC} {audio_name}");
            match stream_download(&client, &a.urls[0], &audio_name, &headers).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{RED}error{NC}: {e}");
                    exit(1);
                }
            }

            println!(" {GREEN}Merging{NC} {name}");
            match FFmpeg::new()
                .arg("-i")
                .arg(&audio_name)
                .arg("-i")
                .arg(&video_name)
                .args(["-c:a", "copy", "-c:v", "copy", "-y"])
                .arg(&name)
                .run()
                .await
            {
                Ok(status) => {
                    if !status.success() {
                        eprintln!(
                            "{RED}error{NC}: ffmpeg exited with status: {:?}",
                            status.code()
                        );
                        exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("{RED}error{NC}: merging video and audio failed: {e}");
                    exit(1);
                }
            }
        } else {
            tokio::fs::copy(video_name, name).await?;
        }

        let pic_path = format!("{key}_pic.jpg");
        println!(" {GREEN}Downloading{NC} {pic_path}");
        match stream_download(&client, &pic, &pic_path, &headers).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("{RED}error{NC}: {e}");
                exit(1);
            }
        }
    }

    Ok(())
}

struct Video {
    id: i32,
    size: u32,
    urls: Vec<String>,
    codecs: Option<String>,
    codecid: Option<i32>,
}

struct Audio {
    id: i32,
    size: u32,
    urls: Vec<String>,
}

fn print_stream(
    pad: &str,
    id: i32,
    codecid: Option<i32>,
    size: u64,
    codecs: Option<String>,
    accept_quality: &[i32],
    accept_description: &[String],
) {
    let mut quality = accept_quality
        .iter()
        .position(|x| id == *x)
        .and_then(|index| accept_description.get(index))
        .cloned()
        .unwrap_or(String::from("高清 720P"));
    if let Some(c) = codecs {
        quality.push(' ');
        quality.push_str(&c);
    }
    let size = BinaryBytes(size);
    println!(
        "{pad}{BLUE}[{}{}]{NC}",
        id,
        codecid.map(|x| format!("-{x}")).unwrap_or_default()
    );
    println!("{pad}{CYAN}{}{NC}  {}", align_left("Quality:", 10), quality);
    println!("{pad}{CYAN}{}{NC}  {}", align_left("Size:", 10), size);
    println!();
}
