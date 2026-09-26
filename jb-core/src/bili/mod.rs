mod abv;
pub mod types;

use std::path::{Path, PathBuf};

use jiff::Timestamp;
use serde::de::DeserializeOwned;
use tokio::fs;
use tracing::{error, info, warn};
use wreq::header::{HeaderMap, HeaderName, HeaderValue, InvalidHeaderValue};
use wreq::{Client, StatusCode};

use self::types::{
    BiliDetail, BiliError, BiliInfo, BiliResult, DescItem, FINGER_HOST, FingerResult,
    GAIA_VALIDATE_HOST, GAIA_VGATE_HOST, GaiaValidateData, GaiaValidateResult, GaiaVgateResult,
    INFO_HOST, MIXIN_KEY_ENC_TAB, NAV_HOST, NavResult, PLAYURL_HOST, PlayurlInfo, QN, VgateData,
    WbiImg,
};

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

pub async fn fetch<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    query: &mut Vec<(&str, String)>,
    grisk_id: Option<String>,
    cookies: &mut Vec<(&str, String)>,
    headers: impl IntoIterator<Item = (&'static str, String)>,
    cache_file: &PathBuf,
) -> Result<T, BiliError> {
    let mixin_key = get_mixin_key(&client).await?;
    let buvid = get_buvid(client).await;
    if let Some((ref b3, _)) = buvid {
        let now = jiff::Timestamp::now();
        let session = format!("{}{}", b3, now.as_millisecond());
        let session = format!("{:x}", md5::compute(session));
        query.push(("session", session));
    }
    wbi(query, &mixin_key);

    if let Some((b3, b4)) = buvid {
        cookies.push(("buvid3", b3));
        cookies.push(("buvid4", b4));
    }
    if let Some(g) = grisk_id {
        cookies.push(("x-bili-gaia-vtoken", g.to_string()));
    }
    let cookie: Vec<String> = cookies
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let cookie: String = cookie.join("; ");

    let headers = headers
        .into_iter()
        .map(|(key, value)| Ok((HeaderName::from_static(key), HeaderValue::from_str(&value)?)))
        .collect::<Result<HeaderMap, InvalidHeaderValue>>()?;
    let response = client
        .get(url)
        .query(&query)
        .headers(headers)
        .header("cookie", cookie)
        .send()
        .await?;
    let status = response.status();
    if status != StatusCode::OK {
        return Err(BiliError::Status(status.as_u16()));
    }

    let header_voucher = response.headers().get("x-bili-gaia-vvoucher").cloned();
    let BiliResult {
        code,
        message,
        data,
    } = response.json().await?;
    match code {
        0 | -352 => {
            if let Some(v_voucher) = header_voucher {
                return Err(BiliError::Voucher(v_voucher.to_str().unwrap().to_string()));
            } else if let Some(v_voucher) = data["v_voucher"].as_str() {
                return Err(BiliError::Voucher(v_voucher.to_string()));
            }
        }
        -404 | 62002 | 62004 => {
            info!(message, "not found");
            return Err(BiliError::NotFound);
        }
        412 => {
            return Err(BiliError::RiskControl);
        }
        _ => {
            error!(code, message, "未知状态码");
            return Err(BiliError::Api(format!("未知状态码 {code}")));
        }
    }

    let pretty = serde_json::to_string_pretty(&data)?;
    if let Err(e) = fs::write(&cache_file, &pretty).await {
        error!("缓存json文件失败: {e:?}");
    } else {
        info!("写入缓存: {}", cache_file.display());
    }

    Ok(serde_json::from_value(data)?)
}

pub async fn fetch_bili_info(
    client: &Client,
    aid: u64,
    bvid: &str,
    grisk_id: Option<String>,
    cookies: &mut Vec<(&str, String)>,
    cache_path: &Path,
) -> Result<BiliInfo, BiliError> {
    let cache_file = cache_path.join(&format!("{bvid}.json"));
    let cache: Option<BiliDetail> = match fs::read_to_string(&cache_file).await {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(e) => {
                warn!("缓存解析失败: {e}");
                None
            }
        },
        Err(_) => None,
    };
    let res: BiliDetail = if let Some(res) = cache {
        info!("使用缓存: {}", cache_file.display());
        res
    } else {
        let mut query = vec![
            ("aid", aid.to_string()),
            ("need_view", String::from("1")),
            ("isGaiaAvoided", String::from("false")),
            ("web_location", String::from("1315873")),
        ];
        if let Some(ref g) = grisk_id {
            query.push(("gaia_vtoken", g.to_string()));
        }

        let referer = format!("https://www.bilibili.com/video/{}/", bvid);
        let headers = [("referer", referer)];

        fetch(
            client,
            INFO_HOST,
            &mut query,
            grisk_id,
            cookies,
            headers,
            &cache_file,
        )
        .await?
    };

    Ok(res.view)
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
    grisk_id: Option<String>,
    cookies: &mut Vec<(&str, String)>,
    cache_path: &Path,
) -> Result<PlayurlInfo, BiliError> {
    let cache_file = cache_path.join(&format!("{bvid}_playurl.json"));

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

    let referer = format!("https://www.bilibili.com/video/{}/", bvid);
    let headers = [("referer", referer)];
    let res: PlayurlInfo = fetch(
        client,
        PLAYURL_HOST,
        &mut query,
        grisk_id,
        cookies,
        headers,
        &cache_file,
    )
    .await?;

    Ok(res)
}

pub async fn get_gaia(
    wreq_client: &wreq::Client,
    v_voucher: String,
) -> Result<VgateData, BiliError> {
    let mut form_data = std::collections::HashMap::new();
    form_data.insert("v_voucher", v_voucher);

    let response = wreq_client
        .post(GAIA_VGATE_HOST)
        .form(&form_data)
        .send()
        .await?;
    let status = response.status();
    if status != StatusCode::OK {
        return Err(BiliError::Status(status.into()));
    }

    let res: GaiaVgateResult = response.json().await?;
    Ok(res.data)
}

pub async fn validate_gaia(
    wreq_client: &wreq::Client,
    token: String,
    challenge: String,
    validate: String,
    seccode: String,
) -> Result<GaiaValidateData, BiliError> {
    let mut form_data = std::collections::HashMap::new();
    form_data.insert("token", token);
    form_data.insert("challenge", challenge);
    form_data.insert("validate", validate);
    form_data.insert("seccode", seccode);

    let response = wreq_client
        .post(GAIA_VALIDATE_HOST)
        .form(&form_data)
        .send()
        .await?;
    let status = response.status();
    if status != StatusCode::OK {
        return Err(BiliError::Status(status.into()));
    }

    let res: GaiaValidateResult = response.json().await?;
    Ok(res.data)
}
