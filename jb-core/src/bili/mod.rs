//! Bilibili

mod abv;
pub mod types;

use std::fmt::Write;
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use rand::distr::Alphanumeric;
use rand::{RngExt, random_range};
use serde::de::DeserializeOwned;
use tokio::fs;
use tracing::{error, info, trace, warn};
use wreq::header::{HeaderMap, HeaderName, HeaderValue, InvalidHeaderValue};
use wreq::{Client, StatusCode};

use self::types::{
    BiliDetail, BiliError, BiliInfo, BiliResult, DescItem, FingerResult, GaiaValidateData,
    GaiaValidateResult, GaiaVgateResult, NavResult, PlayurlInfo, QN, VgateData, WbiImg,
};

/// 获取 buvid
pub async fn get_buvid(client: &Client) -> Option<(String, String)> {
    const FINGER_HOST: &str = "https://api.bilibili.com/x/frontend/finger/spi";

    let response = client.get(FINGER_HOST).send().await.ok()?;
    let res: FingerResult = response.json().await.ok()?;
    let b3 = res.data.b_3;
    let b4 = res.data.b_4;
    Some((b3, b4))
}

/// Calculate mixin_key from `img_key` and `sub_key`.
fn calc_mixin_key(img_key: &str, sub_key: &str) -> String {
    debug_assert_eq!(img_key.len(), 32, "img_key expected 32 length");
    debug_assert_eq!(sub_key.len(), 32, "sub_key expected 32 length");
    debug_assert!(
        img_key.is_ascii() && sub_key.is_ascii(),
        "keys must be ASCII"
    );

    const MIXIN_KEY_ENC_TAB: [u8; 64] = [
        46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19,
        29, 28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4,
        22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
    ];

    let img_bytes = img_key.as_bytes();
    let sub_bytes = sub_key.as_bytes();

    let mut out = Vec::with_capacity(32);

    for &idx in MIXIN_KEY_ENC_TAB.iter().take(32) {
        let byte = if idx < 32 {
            img_bytes[idx as usize]
        } else {
            sub_bytes[(idx - 32) as usize]
        };
        out.push(byte);
    }

    String::from_utf8(out).expect("mixin key should be ASCII")
}

/// 获取 mixin_key
pub async fn get_mixin_key(client: &Client) -> wreq::Result<String> {
    const NAV_HOST: &str = "https://api.bilibili.com/x/web-interface/nav";

    let response = client.get(NAV_HOST).send().await?;
    let res: NavResult = response.json().await?;
    let WbiImg { img_url, sub_url } = res.data.wbi_img;
    let img_key = img_url.rsplit('/').next().unwrap();
    let img_key = img_key.split('.').next().unwrap();
    let sub_key = sub_url.rsplit('/').next().unwrap();
    let sub_key = sub_key.split('.').next().unwrap();
    trace!(img_key, sub_key);

    Ok(calc_mixin_key(img_key, sub_key))
}

fn calc_wbi(query: &[(&str, String)], mixin_key: &str) -> String {
    let mut pairs: Vec<_> = query.iter().collect();
    pairs.sort_unstable_by_key(|x| x.0);

    let mut ser = form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        let v: String = v
            .chars()
            .filter(|c| !matches!(c, '!' | '\'' | '(' | ')' | '*'))
            .collect();
        ser.append_pair(k, &v);
    }
    let encoded = ser.finish();

    let mut ctx = md5::Context::new();
    ctx.consume(encoded.as_bytes());
    ctx.consume(mixin_key.as_bytes());
    format!("{:x}", ctx.finalize())
}

/// 对参数进行 wbi 签名
pub fn wbi(query: &mut Vec<(&str, String)>, mixin_key: &str) {
    query.reserve(2);
    let now = Timestamp::now().as_second();
    query.push(("wts", now.to_string()));

    let w_rid = calc_wbi(query, mixin_key);
    query.push(("w_rid", w_rid));
}

/// 随机生成 dm_img_inter
pub fn random_dm_img_inter() -> String {
    let wh0 = random_range(6500..6800);
    let wh1 = random_range(2200..2300);
    let wh2 = random_range(30..100);
    let of0 = random_range(5..500);
    let of1 = random_range(5..1000);
    let of2 = random_range(5..500);
    format!(
        r#"{{"ds":[{{"t":2,"c":"YnB4LXBsYXllci12aWRlby1pbnB1dGJhci13cm","p":[1710,82,2100],"s":[420,1763,2424]}},{{"t":2,"c":"YnB4LXBsYXllci1kbS1idG4tc2VuZCBidWkgYnVpLWJ1dHRvbg","p":[2365,15,1605],"s":[5,250,130]}}],"wh":[{wh0},{wh1},{wh2}],"of":[{of0},{of1},{of2}]}})"#
    )
}

/// 随机生成 dm_img_str, dm_cover_img_str, dm_img_list, dm_img_inter
pub fn random_dm() -> Vec<(&'static str, String)> {
    let mut dm_img_list = String::from("[");
    let mut timestamp = random_range(3000..4000);
    for i in 0..3 {
        if i > 0 {
            dm_img_list.push(',');
        }
        let x = random_range(50..2500);
        let y = random_range(-50..500);
        let z = random_range(0..100);
        let k = random_range(50..150);
        let t = random_range(0..=1);
        write!(
            dm_img_list,
            r#"{{"x":{x},"y":{y},"z":{z},"timestamp":{timestamp},"k":{k},"type":{t}}}"#
        )
        .unwrap();
        timestamp += random_range(1000..2000);
    }
    dm_img_list.push(']');

    let dm_img_str = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(46)
        .map(char::from)
        .collect();
    let dm_cover_img_str = rand::rng()
        .sample_iter(&Alphanumeric)
        .take(74)
        .map(char::from)
        .collect();

    vec![
        ("dm_img_list", dm_img_list),
        ("dm_img_str", dm_img_str),
        ("dm_cover_img_str", dm_cover_img_str),
        ("dm_img_inter", random_dm_img_inter()),
    ]
}

/// bilibili 请求
pub async fn fetch<T: DeserializeOwned, K, V>(
    client: &Client,
    url: &str,
    query: &mut Vec<(&str, String)>,
    grisk_id: Option<&str>,
    cookies: &[(K, V)],
    headers: impl IntoIterator<Item = (&'static str, String)>,
    cache_file: &PathBuf,
) -> Result<T, BiliError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    let mixin_key = get_mixin_key(client).await?;
    let buvid = get_buvid(client).await;

    if let Some((ref b3, _)) = buvid {
        let now = jiff::Timestamp::now();
        let session = format!("{}{}", b3, now.as_millisecond());
        let session = format!("{:x}", md5::compute(session));
        query.push(("session", session));
    }

    if let Some(g) = grisk_id {
        query.push(("gaia_vtoken", g.to_string()));
    }

    query.extend([
        (
            "x-bili-device-req-json",
            r#"{"platform":"web","device":"pc","mobi_app":"web_cn"}"#.to_string(),
        ),
        (
            "x-bili-locale-json",
            r#"{"c_locale":{"language":"zh","script":"Hans"},"always_translate":false}"#
                .to_string(),
        ),
    ]);
    wbi(query, &mixin_key);

    let mut extra: Vec<(&str, String)> = Vec::new();
    if let Some((b3, b4)) = buvid {
        extra.push(("buvid3", b3));
        extra.push(("buvid4", b4));
    }
    if let Some(g) = grisk_id {
        extra.push(("x-bili-gaia-vtoken", g.to_string()));
    }
    let cookie = crate::encode_cookies(
        cookies
            .iter()
            .map(|(k, v)| (k.as_ref(), v.as_ref()))
            .chain(extra.iter().map(|(k, v)| (*k, v.as_str()))),
    );

    let headers = headers
        .into_iter()
        .map(|(key, value)| Ok((HeaderName::from_static(key), HeaderValue::from_str(&value)?)))
        .collect::<Result<HeaderMap, InvalidHeaderValue>>()?;

    let response = client
        .get(url)
        .query(query)
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
            } else if let Some(ref d) = data
                && let Some(v_voucher) = d["v_voucher"].as_str()
            {
                return Err(BiliError::Voucher(v_voucher.to_string()));
            }
        }
        -404 | 62002 | 62004 => {
            info!(url, message, "not found");
            if message == "啥都木有" {
                return Err(BiliError::Api(
                    "视频不存在，可能是带有地区限制的视频".to_string(),
                ));
            } else {
                return Err(BiliError::NotFound);
            }
        }
        412 => {
            return Err(BiliError::RiskControl);
        }
        _ => {
            error!(code, message, "Unknown code");
            return Err(BiliError::Api(format!("Unknown code: {code}")));
        }
    }

    if let Some(d) = data {
        let pretty = serde_json::to_string_pretty(&d)?;
        if let Err(e) = fs::write(&cache_file, &pretty).await {
            error!("Failed to write cache json: {e:?}");
        } else {
            info!("Write cache: {}", cache_file.display());
        }

        Ok(serde_json::from_value(d)?)
    } else {
        Err(BiliError::Api("No data".to_string()))
    }
}

/// 获取 Bilibili 视频详细信息. 需要 wbi 签名. bilibili 网页版播放器使用的 api.
pub async fn fetch_bili_detail<K, V>(
    client: &Client,
    aid: u64,
    bvid: &str,
    grisk_id: Option<&str>,
    cookies: &[(K, V)],
    cache_path: &Path,
) -> Result<BiliInfo, BiliError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    const DETAIL_HOST: &str = "https://api.bilibili.com/x/web-interface/wbi/view/detail";

    let cache_file = cache_path.join(format!("{bvid}.json"));
    let cache: Option<BiliDetail> = match fs::read_to_string(&cache_file).await {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(e) => {
                warn!("Failed to parse cached json: {e}");
                None
            }
        },
        Err(_) => None,
    };
    let res: BiliDetail = if let Some(res) = cache {
        info!("Using cached json: {}", cache_file.display());
        res
    } else {
        let mut query = vec![
            ("aid", aid.to_string()),
            ("need_view", String::from("1")),
            ("isGaiaAvoided", String::from("false")),
            ("web_location", String::from("1315873")),
        ];

        query.push(("dm_img_list", String::from("[]")));
        // base64_encode("WebGL 1.0 (OpenGL ES 2.0 Chromium)")[0..len-2]
        query.push((
            "dm_img_str",
            String::from("V2ViR0wgMS4wIChPcGVuR0wgRVMgMi4wIENocm9taXVtKQ"),
        ));
        // base64_encode("ANGLE (ARM, Mali-G57 MC2, OpenGL ES 3.2)Google Inc. (ARM)")[0..len-2]
        query.push((
            "dm_cover_img_str",
            String::from(
                "QU5HTEUgKEFSTSwgTWFsaS1HNTcgTUMyLCBPcGVuR0wgRVMgMy4yKUdvb2dsZSBJbmMuIChBUk",
            ),
        ));
        query.push(("dm_img_inter", random_dm_img_inter()));

        let referer = format!("https://www.bilibili.com/video/{}/", bvid);
        let headers = [("referer", referer)];

        fetch(
            client,
            DETAIL_HOST,
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

/// 获取 Bilibili 视频信息. 需要 wbi 签名
pub async fn fetch_bili_view<K, V>(
    client: &Client,
    aid: u64,
    bvid: &str,
    grisk_id: Option<&str>,
    cookies: &[(K, V)],
    cache_path: &Path,
) -> Result<BiliInfo, BiliError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    const VIEW_HOST: &str = "https://api.bilibili.com/x/web-interface/wbi/view";

    let cache_file = cache_path.join(format!("{bvid}.json"));
    let cache: Option<BiliInfo> = match fs::read_to_string(&cache_file).await {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(e) => {
                warn!("Failed to parse cached json: {e}");
                None
            }
        },
        Err(_) => None,
    };
    let res: BiliInfo = if let Some(res) = cache {
        info!("Using cached json: {}", cache_file.display());
        res
    } else {
        let mut query = vec![
            ("aid", aid.to_string()),
            ("isGaiaAvoided", String::from("false")),
            ("web_location", String::from("1315873")),
        ];

        query.push(("dm_img_list", String::from("[]")));
        // base64_encode("WebGL 1.0 (OpenGL ES 2.0 Chromium)")[0..len-2]
        query.push((
            "dm_img_str",
            String::from("V2ViR0wgMS4wIChPcGVuR0wgRVMgMi4wIENocm9taXVtKQ"),
        ));
        // base64_encode("ANGLE (ARM, Mali-G57 MC2, OpenGL ES 3.2)Google Inc. (ARM)")[0..len-2]
        query.push((
            "dm_cover_img_str",
            String::from(
                "QU5HTEUgKEFSTSwgTWFsaS1HNTcgTUMyLCBPcGVuR0wgRVMgMy4yKUdvb2dsZSBJbmMuIChBUk",
            ),
        ));
        query.push(("dm_img_inter", random_dm_img_inter()));

        let referer = format!("https://www.bilibili.com/video/{}/", bvid);
        let headers = [("referer", referer)];

        fetch(
            client,
            VIEW_HOST,
            &mut query,
            grisk_id,
            cookies,
            headers,
            &cache_file,
        )
        .await?
    };

    Ok(res)
}

/// 获取 Bilibili 视频信息. 无需 wbi 签名. 该 api 不会检查视频的地区限制
pub async fn fetch_bili_info<K, V>(
    client: &Client,
    aid: u64,
    bvid: &str,
    cookies: &[(K, V)],
    cache_path: &Path,
) -> Result<BiliInfo, BiliError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    const PLAYER_VIEW_HOST: &str = "https://api.bilibili.com/x/player/view";

    let cache_file = cache_path.join(format!("{bvid}.json"));
    let cache: Option<BiliInfo> = match fs::read_to_string(&cache_file).await {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(value) => Some(value),
            Err(e) => {
                warn!("Failed to parse cached json: {e}");
                None
            }
        },
        Err(_) => None,
    };
    let res: BiliInfo = if let Some(res) = cache {
        info!("Using cached json: {}", cache_file.display());
        res
    } else {
        let mut query = vec![("aid", aid.to_string())];
        let referer = format!("https://www.bilibili.com/video/{}/", bvid);
        let headers = [("referer", referer)];

        fetch(
            client,
            PLAYER_VIEW_HOST,
            &mut query,
            None,
            cookies,
            headers,
            &cache_file,
        )
        .await?
    };

    Ok(res)
}

/// 解析简介
pub fn parse_desc(desc: &[DescItem]) -> String {
    if desc.is_empty() {
        return String::new();
    }
    desc.iter()
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

/// 获取视频播放流
pub async fn fetch_playurl<K, V>(
    client: &Client,
    aid: u64,
    bvid: &str,
    cid: i64,
    grisk_id: Option<&str>,
    cookies: &[(K, V)],
    cache_path: &Path,
) -> Result<PlayurlInfo, BiliError>
where
    K: AsRef<str>,
    V: AsRef<str>,
{
    const PLAYURL_HOST: &str = "https://api.bilibili.com/x/player/wbi/playurl";

    let cache_file = cache_path.join(format!("{bvid}_playurl.json"));

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
        ("version_name", "4.10.4".to_string()),
        ("app_id", "100".to_string()),
        ("voice_balance", "1".to_string()),
        ("try_look", "1".to_string()),
        ("web_location", "1315873".to_string()),
    ];
    query.extend(random_dm());

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

/// 获取人机验证信息
pub async fn get_gaia(
    wreq_client: &wreq::Client,
    v_voucher: String,
) -> Result<VgateData, BiliError> {
    const GAIA_VGATE_HOST: &str = "https://api.bilibili.com/x/gaia-vgate/v1/register";

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

/// 进行人机验证
pub async fn validate_gaia(
    wreq_client: &wreq::Client,
    token: String,
    challenge: String,
    validate: String,
    seccode: String,
) -> Result<GaiaValidateData, BiliError> {
    const GAIA_VALIDATE_HOST: &str = "https://api.bilibili.com/x/gaia-vgate/v1/validate";

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

#[cfg(test)]
mod tests {
    #[test]
    fn test_calc_mixin_key() {
        let mixin_key = super::calc_mixin_key(
            "7cd084941338484aae1ad9425b84077c",
            "4932caff0ff746eab6f01bf08b70ac45",
        );
        assert_eq!(&mixin_key, "ea1db124af3c7062474693fa704f4ff8");
    }

    #[test]
    fn test_wbi() {
        let query = &[
            ("aid", "117431653635609".to_string()),
            ("isGaiaAvoided", "false".to_string()),
            ("web_location", "1315873".to_string()),
            ("dm_img_list", "[]".to_string()),
            ("wts", "1792066128".to_string()),
        ];
        let w_rid = super::calc_wbi(query, "ea1db124af3c7062474693fa704f4ff8");
        assert_eq!(&w_rid, "74e75d8b0f71108f9f7401949183e07f");
    }
}
