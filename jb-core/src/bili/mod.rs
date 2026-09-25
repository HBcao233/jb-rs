mod abv;
pub mod types;

use std::path::Path;

use jiff::Timestamp;
use serde_json::Value;
use tokio::fs;
use tracing::{error, info};
use wreq::Client;
use wreq::StatusCode;

use self::types::{
    BiliInfo, BiliResult, DescItem, GetBiliError, GetPlayurlError, INFO_HOST, PLAYURL_HOST,
    PlayurlInfo, PlayurlResult, QN,
};
use self::types::{FINGER_HOST, FingerResult, MIXIN_KEY_ENC_TAB, NAV_HOST, NavResult, WbiImg};

pub async fn get_buvid(client: &Client) -> Option<(String, String)> {
    let response = client.get(FINGER_HOST).send().await.ok()?;
    let res: FingerResult = response.json().await.ok()?;
    let b3 = res.data.b_3;
    let b4 = res.data.b_4;
    Some((b3, b4))
}

pub async fn get_mixin_key(client: &Client) -> wreq::Result<String> {
    let response = client.get(NAV_HOST).send().await?;
    let res: NavResult = response.json().await?;
    let WbiImg { img_url, sub_url } = res.data.wbi_img;
    let img_key = img_url.rsplit('/').next().unwrap();
    let img_key = img_key.split('.').next().unwrap();
    let sub_key = sub_url.rsplit('/').next().unwrap();
    let sub_key = sub_key.split('.').next().unwrap();

    let orig: Vec<char> = img_key.chars().chain(sub_key.chars()).collect();
    let mixin_key: String = MIXIN_KEY_ENC_TAB.iter().fold(String::new(), |mut acc, i| {
        acc.push(*orig.get(*i as usize).unwrap());
        acc
    });
    Ok(mixin_key.chars().take(32).collect())
}

pub fn wbi(query: &mut Vec<(&'_ str, String)>, mixin_key: &str) {
    let now = Timestamp::now();
    let now = now.as_second().to_string();
    query.push(("wts", now));

    let mut q = query.clone();
    q.sort_by_key(|x| x.0);
    let iter = q.into_iter().map(|(k, mut v)| {
        v.retain(|c| !matches!(c, '!' | '\'' | '(' | ')' | '*'));
        (k, v)
    });
    let encoded: String = form_urlencoded::Serializer::new(String::new())
        .extend_pairs(iter)
        .finish();
    let w_rid = format!("{:x}", md5::compute(encoded + mixin_key));

    query.push(("w_rid", w_rid));
}

pub async fn fetch_bili_info(
    client: &Client,
    aid: u64,
    bvid: &str,
    cache_path: &Path,
) -> Result<BiliInfo, GetBiliError> {
    let cache_file = cache_path.join(&format!("{bvid}.json"));
    let res: BiliResult = if let Ok(text) = fs::read_to_string(&cache_file).await {
        info!("使用缓存: {}", cache_file.display());
        serde_json::from_str(&text)?
    } else {
        let mixin_key = get_mixin_key(&client).await?;
        let mut query = vec![
            ("aid", aid.to_string()),
            ("need_view", String::from("1")),
            ("isGaiaAvoided", String::from("false")),
            ("web_location", String::from("1315873")),
        ];
        wbi(&mut query, &mixin_key);

        let buvid = get_buvid(client).await;
        let cookie: String = {
            let mut c = Vec::new();
            if let Some((b3, b4)) = buvid {
                c.push(("buvid3", b3));
                c.push(("buvid4", b4));
            }
            let c: Vec<String> = c.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
            c.join("; ")
        };

        let referer = format!("https://www.bilibili.com/video/{}/", bvid);
        let response = client
            .get(INFO_HOST)
            .query(&query)
            .header("cookie", cookie)
            .header("referer", referer)
            .send()
            .await?;
        let status = response.status();
        if status != StatusCode::OK {
            return Err(GetBiliError::Status(status.as_u16()));
        }

        let header_voucher = response.headers().get("x-bili-gaia-vvoucher").cloned();
        let res: Value = response.json().await?;
        match res["code"].as_i64().unwrap() {
            -404 | 62002 | 62004 | 0 | -352 => {
                if let Some(v_voucher) = header_voucher {
                    return Err(GetBiliError::Voucher(
                        v_voucher.to_str().unwrap().to_string(),
                    ));
                } else if let Some(v_voucher) = res["data"]["v_voucher"].as_str() {
                    return Err(GetBiliError::Voucher(v_voucher.to_string()));
                } else {
                    let pretty = serde_json::to_string_pretty(&res)?;
                    if let Err(e) = fs::write(&cache_file, &pretty).await {
                        error!("缓存json文件失败: {e:?}");
                    } else {
                        info!("写入缓存: {}", cache_file.display());
                    }
                }
            }
            _ => {}
        }

        serde_json::from_value(res)?
    };

    match res.code {
        -404 | 62002 | 62004 => {
            info!("{}", res.message);
            return Err(GetBiliError::NotFound);
        }
        0 | -352 => {}
        412 => {
            return Err(GetBiliError::RiskControl);
        }
        _ => {
            let msg = format!("未知状态码: {} {}", res.code, res.message);
            error!("{msg}");
            return Err(GetBiliError::Api(msg));
        }
    }

    match res.data.view {
        Some(view) => Ok(view),
        None => {
            return Err(GetBiliError::RiskControl);
        }
    }
}

pub fn parse_desc(desc: &[DescItem]) -> String {
    if desc.is_empty() {
        return String::new();
    }
    desc.into_iter()
        .map(|d| {
            if d.r#type == 2 {
                format!("[{}](https://space.bilibili.com/{})", d.raw_text, d.biz_id)
            } else {
                d.raw_text.clone()
            }
        })
        .collect::<Vec<String>>()
        .join("")
}

pub async fn fetch_playurl(
    client: &Client,
    aid: u64,
    bvid: &str,
    cid: i64,
    cache_path: &Path,
) -> Result<PlayurlInfo, GetPlayurlError> {
    let cache_file = cache_path.join(&format!("{bvid}_playurl.json"));

    let mixin_key = get_mixin_key(&client).await?;

    let mut query = vec![
        ("avid", aid.to_string()),
        ("bvid", bvid.to_string()),
        ("cid", cid.to_string()),
        ("qn", QN.to_string()),
        ("fnver", "0".to_string()),
        ("fnval", "4048".to_string()),
        ("fourk", "1".to_string()),
        ("gaia_source", "".to_string()),
        ("from_client", "BROWSER".to_string()),
        ("is_main_page", "true".to_string()),
        ("need_fragment", "false".to_string()),
        ("isGaiaAvoided", "false".to_string()),
        ("client_attr", "0".to_string()),
        ("version_name", "4.9.96-rc.5539.0".to_string()),
        ("app_id", "100".to_string()),
        ("voice_balance", "1".to_string()),
        ("try_look", "1".to_string()),
        ("web_location", "1315873".to_string()),
    ];
    let buvid = get_buvid(client).await;
    if let Some((ref b3, _)) = buvid {
        let now = jiff::Timestamp::now();
        let session = format!("{}{}", b3, now.as_millisecond());
        let session = format!("{:x}", md5::compute(session));
        query.push(("session", session));
    }
    wbi(&mut query, &mixin_key);

    let cookie: String = {
        let mut c = Vec::new();
        if let Some((b3, b4)) = buvid {
            c.push(("buvid3", b3));
            c.push(("buvid4", b4));
        }
        let c: Vec<String> = c.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
        c.join("; ")
    };

    let referer = format!("https://www.bilibili.com/video/{}/", bvid);
    let request = client
        .get(PLAYURL_HOST)
        .query(&query)
        .header("cookie", cookie)
        .header("referer", referer);

    let response = request.send().await?;
    let status = response.status();
    if status != StatusCode::OK {
        return Err(GetPlayurlError::Status(status.as_u16()));
    }

    let res: Value = response.json().await?;

    let pretty = serde_json::to_string_pretty(&res)?;
    if let Err(e) = fs::write(&cache_file, &pretty).await {
        error!("缓存json文件失败: {e:?}");
    } else {
        info!("写入缓存: {}", cache_file.display());
    }
    let res: PlayurlResult = serde_json::from_value(res)?;

    if let Some(v_voucher) = res.data.v_voucher {
        return Err(GetPlayurlError::Voucher(v_voucher));
    }
    Ok(res.data)
}
