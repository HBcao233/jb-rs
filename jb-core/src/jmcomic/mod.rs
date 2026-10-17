pub mod types;

use aes::cipher::{BlockModeDecrypt, KeyInit, block_padding::Pkcs7};
use base64::prelude::{BASE64_STANDARD, Engine as _};
use jiff::Timestamp;
use serde::Serialize;
use serde::de::DeserializeOwned;
use tracing::{debug, info, warn};
use wreq::{Client, Response, Uri, header};

use self::types::{AlbumInfo, DomainResult, JMError, JMResult};

const UA: &str = "Mozilla/5.0 (Linux; Android 9; V1938CT Build/PQ3A.190705.11211812; wv) AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/91.0.4472.114 Safari/537.36";

const VERSION: &str = "2.0.26";
const SALT: &str = "185Hcomic3PAPP7R";

// md5("diosfjckwpqpdfjkvnqQjsik")
const DOMAIN_KEY: [u8; 32] = *b"6ae6b037507edb82ee4699827c4b4066";

const DOMAIN_SERVER_URLS: [&str; 3] = [
    "https://rup4a04-c01.tos-ap-southeast-1.bytepluses.com/newsvr-2025.txt",
    "https://rup4a04-c02.tos-cn-hongkong.bytepluses.com/newsvr-2025.txt",
    "https://rup4a04-c03.tos-cn-beijing.bytepluses.com.cn/newsvr-2025.txt",
];

const API_DOMAINS: [&str; 5] = [
    "www.cdnhjk.net",
    "www.cdngwc.cc",
    "www.cdngwc.net",
    "www.cdngwc.club",
    "www.cdnutc.me",
];

pub const CDN_DOMAINS: [&str; 6] = [
    "cdn-msp.jmapiproxy1.cc",
    "cdn-msp.jmapiproxy2.cc",
    "cdn-msp2.jmapiproxy2.cc",
    "cdn-msp3.jmapiproxy2.cc",
    "cdn-msp.jmapinodeudzn.net",
    "cdn-msp3.jmapinodeudzn.net",
];

fn decrypt<T: DeserializeOwned>(encoded: &str, key: [u8; 32]) -> Result<T, JMError> {
    let mut data = BASE64_STANDARD.decode(encoded.trim())?;
    let cipher = ecb::Decryptor::<aes::Aes256>::new((&key).into());
    let res = cipher.decrypt_padded::<Pkcs7>(&mut data)?;
    Ok(serde_json::from_slice(&res)?)
}

pub fn decrypt_data<T: DeserializeOwned>(encoded: &str, ts: &str) -> Result<T, JMError> {
    let mut ctx = md5::Context::new();
    ctx.consume(ts.as_bytes());
    ctx.consume(SALT.as_bytes());
    let key = format!("{:x}", ctx.finalize());
    decrypt(encoded, key.as_bytes().try_into().unwrap())
}

pub async fn resolve_domains(client: &Client) -> Vec<String> {
    for url in DOMAIN_SERVER_URLS {
        if let Ok(response) = client.get(url).send().await
            && let Ok(text) = response.text().await
        {
            match decrypt(text.trim(), DOMAIN_KEY) {
                Ok(DomainResult { jm3_Server }) => {
                    return jm3_Server
                        .into_iter()
                        .map(|x| x.into_iter().next().unwrap())
                        .collect();
                }
                Err(e) => warn!("base64 decode failed: {e}"),
            }
        }
    }

    API_DOMAINS.iter().map(|s| s.to_string()).collect()
}

async fn fetch_raw<Q: Serialize + ?Sized>(
    client: &Client,
    domain: &str,
    path: &str,
    query: &Q,
) -> Result<(String, Response), JMError> {
    let ts = Timestamp::now().as_second().to_string();
    info!(ts);
    let mut ctx = md5::Context::new();
    ctx.consume(ts.as_bytes());
    ctx.consume(SALT.as_bytes());
    let token = format!("{:x}", ctx.finalize());
    let tokenparam = format!("{},{}", ts, VERSION);

    let uri = Uri::builder()
        .scheme("https")
        .authority(domain)
        .path_and_query(path)
        .build()
        .unwrap();
    let response = client
        .get(uri)
        .query(query)
        .header(header::USER_AGENT, UA)
        .header("token", &token)
        .header("tokenparam", tokenparam)
        .send()
        .await?;

    let status = response.status();
    if !status.is_success() {
        return Err(JMError::Status(status.as_u16()));
    }

    Ok((token, response))
}

async fn call_raw<Q: Serialize + ?Sized>(
    client: &Client,
    path: &str,
    query: &Q,
) -> Result<(String, Response), JMError> {
    for domain in API_DOMAINS {
        info!(domain, path);
        match fetch_raw(client, domain, path, query).await {
            Ok((token, res)) => return Ok((token, res)),
            Err(e) => match e {
                JMError::Json(_)
                | JMError::Base64(_)
                | JMError::Aes(_)
                | JMError::NotFound
                | JMError::Api(_) => return Err(e),
                _ => {}
            },
        }
    }
    Err(JMError::Api("请求失败".to_string()))
}

async fn call_json<Q: Serialize + ?Sized, T: DeserializeOwned>(
    client: &Client,
    path: &str,
    query: &Q,
) -> Result<T, JMError> {
    for domain in API_DOMAINS {
        info!(domain, path);
        match fetch_raw(client, domain, path, query).await {
            Ok((token, response)) => {
                let JMResult { code, data } = response.json().await?;
                debug!(code);
                if code != 200 {
                    return Err(JMError::Api(format!("code error: {code}")));
                }

                return match data {
                    serde_json::Value::String(s) => {
                        decrypt(&s, token.as_bytes().try_into().unwrap())
                    }
                    serde_json::Value::Array(_) => Err(JMError::NotFound),
                    _ => Err(JMError::Api("解析失败".to_string())),
                };
            }
            Err(e) => match e {
                JMError::Json(_)
                | JMError::Base64(_)
                | JMError::Aes(_)
                | JMError::NotFound
                | JMError::Api(_) => return Err(e),
                _ => {}
            },
        }
    }

    Err(JMError::Api("请求失败".to_string()))
}

pub async fn fetch_album_info(client: &Client, album_id: i32) -> Result<AlbumInfo, JMError> {
    call_json(client, "/album", &[("id", album_id)]).await
}

pub async fn fetch_scramble_id(client: &Client, photo_id: i32) -> Result<i32, JMError> {
    use futures_util::stream::StreamExt;

    let query = &[
        ("id", photo_id.to_string()),
        ("mode", "vertical".to_string()),
        ("page", "0".to_string()),
        ("app_img_shunt", "1".to_string()),
    ];
    let (_, response) = call_raw(client, "/chapter_view_template", query).await?;

    let mut stream = response.bytes_stream();

    let mut html = Vec::with_capacity(2048);

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        html.extend_from_slice(&chunk);

        if html.len() >= 2048 {
            break;
        }
    }

    let text = String::from_utf8_lossy(&html);

    const PREFIX: &str = "var scramble_id = ";
    if let Some(start) = text.find(PREFIX) {
        let content_start = start + PREFIX.len();

        if let Some(end) = text[content_start..].find(';') {
            return Ok(text[content_start..content_start + end].parse()?);
        }
    }

    Err(JMError::Api(String::from("解析失败")))
}

pub async fn fetch_chapter(client: &Client, photo_id: i32) -> Result<serde_json::Value, JMError> {
    call_json(client, "/chapter", &[("id", photo_id)]).await
}
