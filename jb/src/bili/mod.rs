use std::borrow::Cow;
use std::fmt::Write;
use std::path::PathBuf;
use std::process::exit;

use indicatif::BinaryBytes;
use jb_core::bili::types::{self, BiliId};
use jb_core::bili::{fetch_bili_info, fetch_playurl, parse_desc};
use regex::{Regex, regex};
use tokio::fs;
use tracing::{error, info};
use url::Url;

use crate::curl::{get_client, stream_download};
use crate::ffmpeg::FFmpeg;
use crate::{
    BLUE, CYAN, GREEN, NC, Options, RED, YELLOW, align_left, padding_left, terminal_width,
};

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
                error!("failed to request b23.tv: {e}");
                eprintln!("{RED}error{NC}: failed to parse bilibili short link.");
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
            eprintln!("{RED}error{NC}: failed to parse bilibili short link.");
            exit(400);
        }
    }

    if let Some(caps) = re.captures(&text) {
        let (_, [id]) = caps.extract();
        info!("bilibili video id: {id}");
        let bili_id = if id.starts_with('a') {
            let id = id.strip_prefix("av").unwrap();
            BiliId::AV(id.parse().unwrap())
        } else {
            BiliId::BV(id.to_string())
        };

        let Some((aid, bvid)) = bili_id.to_raw() else {
            eprintln!("{RED}error{NC}: Invalid avid/bvid: {id}");
            exit(400);
        };
        if let Err(e) = parse_bili(aid, bvid, options).await {
            eprintln!("{RED}error{NC}: failed to parse bilibili video id: {e}");
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
    let cache_dir = crate::home_dir().unwrap_or(PathBuf::from(".")).join("bili");
    fs::create_dir_all(&cache_dir).await?;

    let client = get_client().build()?;
    let referer = format!("https://www.bilibili.com/video/{}/", bvid);
    let headers = vec![("referer", referer)];

    let types::BiliInfo {
        title,
        desc_v2,
        pic,
        pages,
        ..
    } = fetch_bili_info(&client, aid, &bvid, None, &options.cookies, &cache_dir).await?;
    println!();
    println!(" {CYAN}{}{NC}  哔哩哔哩 Bilibili", align_left("Site:", 13));
    println!(" {CYAN}{}{NC}  {}", align_left("BVid:", 13), bvid);
    println!(" {CYAN}{}{NC}  {}", align_left("Title:", 13), title);
    let desc = match desc_v2 {
        Some(d) => parse_desc(&d),
        None => String::new(),
    };
    let desc = desc.trim();
    if desc.contains('\n') {
        println!(" {CYAN}Description:{NC}\n {}", padding_left(desc, 1));
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
        eprintln!("{RED}error{NC}: no video with page=`{p}` found.");
        exit(1)
    });

    let playurl =
        fetch_playurl(&client, aid, &bvid, cid, None, &options.cookies, &cache_dir).await?;

    let quality = playurl.quality.unwrap();
    let accept_quality = playurl.accept_quality.unwrap();
    let accept_description = playurl.accept_description.unwrap();

    let mut videos = Vec::new();
    let mut audios = Vec::new();
    let re = regex!(r"https://[^/]*akamaized\.net");
    const HW_CDN: &str = "https://upos-sz-mirrorcos.bilivideo.com";

    if let Some(ref da) = playurl.dash {
        let duration = da.duration;

        for v in da.video.iter() {
            if v.mime_type == "video/mp4" {
                let mut urls = Vec::with_capacity(1 + v.backup_url.len());
                urls.push(re.replace(&v.base_url, HW_CDN));
                urls.extend(v.backup_url.iter().map(|u| re.replace(u, HW_CDN)));
                videos.push(Video {
                    id: v.id,
                    size: v.bandwidth as u64 * duration as u64 / 8,
                    urls,
                    codecs: Some(&v.codecs),
                    codecid: Some(v.codecid),
                });
            }
        }

        if let Some(ref au) = da.audio {
            for v in au.iter() {
                if v.mime_type == "audio/mp4" {
                    let mut urls = Vec::with_capacity(1 + v.backup_url.len());

                    urls.push(re.replace(&v.base_url, HW_CDN));
                    urls.extend(v.backup_url.iter().map(|u| re.replace(u, HW_CDN)));
                    audios.push(Audio {
                        id: v.id,
                        size: v.bandwidth as u64 * duration as u64 / 8,
                        urls,
                    });
                }
            }
        }
    }

    if let Some(ref durl) = playurl.durl {
        let types::DurlInfo {
            url,
            backup_url,
            size,
        } = durl.iter().next().unwrap();
        let mut urls = Vec::with_capacity(1 + backup_url.as_ref().map(|b| b.len()).unwrap_or(0));

        urls.push(re.replace(url, HW_CDN));

        if let Some(backup) = backup_url {
            urls.extend(backup.iter().map(|u| re.replace(u, HW_CDN)));
        }

        videos.push(Video {
            id: quality,
            size: *size as u64,
            urls,
            codecs: None,
            codecid: None,
        });
    }

    if videos.is_empty() {
        eprintln!("{RED}error{NC}: no stream information was parsed.");
        exit(500);
    };
    videos.sort_by(|a, b| {
        // id 降序, codecid 升序
        b.id.cmp(&a.id).then_with(|| a.codecid.cmp(&b.codecid))
    });

    let width = terminal_width().unwrap_or(59) as usize;

    let pad = if width >= 80 { "     " } else { "   " };

    let audio = audios.into_iter().max_by_key(|x| x.id);
    let show_videos = if let Some(ref expected_quality) = options.quality {
        find_expected_quality_video(&videos, expected_quality)
    } else {
        videos.iter().collect()
    };

    if options.info {
        if show_videos.is_empty()
            && let Some(ref expected_quality) = options.quality
        {
            eprintln!("{RED}error{NC}: no videos with quality=`{expected_quality}` found.");
            exit(400);
        }

        println!(" {CYAN}Videos:{NC}");

        for video in show_videos {
            print_stream(
                pad,
                video,
                audio.as_ref(),
                &accept_quality,
                &accept_description,
            );
        }

        exit(0);
    } else {
        let video = if options.quality.is_some() {
            show_videos.into_iter().min_by_key(|v| v.codecid)
        } else {
            videos.first()
        }
        .unwrap_or_else(|| {
            eprintln!(
                "{RED}error{NC}: no video with quality=`{}` found.",
                options.quality.as_ref().unwrap()
            );
            exit(400);
        });
        println!(" {CYAN}Selected video:{NC}");

        print_stream(
            pad,
            video,
            audio.as_ref(),
            &accept_quality,
            &accept_description,
        );

        let key = format!("{bvid}_{cid}");
        let name = format!(
            "{key}_{}+{}.mp4",
            video.id,
            audio.as_ref().map(|a| a.id).unwrap_or_default()
        );

        if !fs::try_exists(&name).await? {
            let video_name = format!(
                "{key}_video_{}{}.mp4",
                video.id,
                if let Some(c) = video.codecid {
                    format!("-{c}")
                } else {
                    String::new()
                }
            );

            let mut urls = video.urls.iter();
            let mut i = 0;
            loop {
                let url = match urls.next() {
                    Some(url) => {
                        if !options.allow_mcdn && url.contains("mcdn.bilivideo") {
                            continue;
                        }

                        let u = Url::parse(url).unwrap();
                        info!("CDN: {}", u.host().unwrap());
                        println!(
                            " {GREEN}Downloading{NC} {video_name}{}",
                            if i == 0 {
                                String::new()
                            } else {
                                format!("Retry {i}")
                            }
                        );
                        url
                    }
                    None => {
                        eprintln!("{RED}error{NC}: download failed");
                        exit(1);
                    }
                };
                match stream_download(&client, url, &video_name, &headers).await {
                    Ok(_) => {
                        break;
                    }
                    Err(e) => {
                        eprintln!("{YELLOW}warn{NC}: download try {i} failed: {e}");
                    }
                }
                i += 1;
            }

            println!();
            if let Some(au) = audio {
                let audio_name = format!("{key}_audio_{}.mp4", au.id);

                let mut urls = au.urls.iter();
                let mut i = 0;
                loop {
                    let url = match urls.next() {
                        Some(url) => {
                            if !options.allow_mcdn && url.contains("mcdn.bilivideo") {
                                continue;
                            }

                            let u = Url::parse(url).unwrap();
                            info!("CDN: {}", u.host().unwrap());
                            println!(
                                " {GREEN}Downloading{NC} {audio_name}{}",
                                if i == 0 {
                                    String::new()
                                } else {
                                    format!(" Retry {i}")
                                }
                            );
                            url
                        }
                        None => {
                            eprintln!("{RED}error{NC}: download failed.");
                            exit(1);
                        }
                    };
                    match stream_download(&client, url, &audio_name, &headers).await {
                        Ok(_) => {
                            break;
                        }
                        Err(e) => {
                            eprintln!("{YELLOW}warn{NC}: download try {i} failed: {e}");
                        }
                    }
                    i += 1;
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

                if !options.keep_temps {
                    fs::remove_file(video_name).await?;
                    fs::remove_file(audio_name).await?;
                }
            } else {
                if options.keep_temps {
                    fs::rename(video_name, name).await?;
                } else {
                    fs::copy(video_name, name).await?;
                }
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

            println!(" {GREEN}Download successful{NC}")
        } else {
            println!(" {GREEN}Downloaded.{NC}")
        }
    }

    Ok(())
}

struct Video<'a> {
    id: i32,
    size: u64,
    urls: Vec<Cow<'a, str>>,
    codecs: Option<&'a str>,
    codecid: Option<i32>,
}

struct Audio<'a> {
    id: i32,
    size: u64,
    urls: Vec<Cow<'a, str>>,
}

fn print_stream(
    pad: &str,
    video: &Video,
    audio: Option<&Audio>,
    accept_quality: &[i32],
    accept_description: &[String],
) {
    let mut size = video.size;
    if let Some(a) = audio {
        size += a.size;
    }

    let mut quality = accept_quality
        .iter()
        .position(|x| *x == video.id)
        .and_then(|index| accept_description.get(index))
        .cloned()
        .unwrap_or(String::from("高清 720P"));
    if let Some(c) = video.codecs {
        quality.push(' ');
        quality.push_str(c);
    }
    let size = BinaryBytes(size);
    let mut q = video.id.to_string();
    if let Some(codecid) = video.codecid {
        q.push('-');
        write!(q, "{codecid}").unwrap();
    }

    println!("{pad}{BLUE}[{}]{NC}", q);
    println!("{pad}{CYAN}{}{NC}  {}", align_left("Quality:", 10), quality);
    println!("{pad}{CYAN}{}{NC}  {}", align_left("Size:", 10), size);
    println!(
        "{pad}{CYAN}{}{NC}  jb -q {} ...",
        align_left("# Download with", 10),
        q
    );
    println!();
}

fn parse_expected_quality(s: &str) -> Result<(i32, Option<i32>), &'static str> {
    let (id_part, code_part) = match s.split_once('-') {
        Some((id, code)) => (id, Some(code)),
        None => (s, None),
    };

    let id: i32 = id_part.parse().map_err(|_| "invalid id")?;
    if id < 0 {
        return Err("id must be non-negative");
    }

    let codecid = match code_part {
        Some(code) => {
            let c: i32 = code.parse().map_err(|_| "invalid codecid")?;
            if c < 0 {
                return Err("codecid must be non-negative");
            }
            Some(c)
        }
        None => None,
    };

    Ok((id, codecid))
}

fn find_expected_quality_video<'a>(
    videos: &'a [Video],
    expected_quality: &str,
) -> Vec<&'a Video<'a>> {
    let (id, codecid) = match parse_expected_quality(expected_quality) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{RED}error{NC}: {e}");
            exit(400);
        }
    };

    videos
        .iter()
        .filter(|v| v.id == id && codecid.is_none_or(|c| v.codecid == Some(c)))
        .collect()
}
