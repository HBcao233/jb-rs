mod types;

use std::env;
use std::path::Path;
use std::sync::OnceLock;

use futures_util::StreamExt;
use jiff::Timestamp;
use rand::RngExt;
use tracing::{trace, warn};
use wreq::Client;
use wreq::multipart::Form;

use self::types::{PICGO_HOST, POSTIMAGE_HOST, PicgoResult, PostimageResult, UploadError};

static PICGO_API_KEY: OnceLock<Option<String>> = OnceLock::new();
static PICGO_ALBUM_ID: OnceLock<Option<String>> = OnceLock::new();

const OG_IMAGE_PREFIX: &str = "<meta property=\"og:image\" content=\"";

#[crate::on_setup]
fn setup() -> anyhow::Result<()> {
    let picgo_api_key = env::var("picgo_api_key").ok();
    PICGO_API_KEY
        .set(picgo_api_key)
        .expect("Must be empty before initialization.");

    let picgo_album_id = env::var("picgo_album_id").ok();
    PICGO_ALBUM_ID
        .set(picgo_album_id)
        .expect("Must be empty before initialization.");

    Ok(())
}

pub async fn get_url(path: impl AsRef<Path>) -> Result<String, UploadError> {
    let client = crate::core::curl::get_client().build()?;
    if let Some(picgo_api_key) = PICGO_API_KEY.get().expect("Must be initialized.") {
        trace!("使用 picgo 上传");
        let picgo_album_id = PICGO_ALBUM_ID
            .get()
            .expect("Must be initialized.")
            .as_deref();
        upload_picgo(&client, picgo_api_key, picgo_album_id, path).await
    } else {
        trace!("使用 postimage 上传");
        upload_postimage(&client, path).await
    }
}

async fn upload_picgo(
    client: &Client,
    picgo_api_key: &str,
    picgo_album_id: Option<&str>,
    path: impl AsRef<Path>,
) -> Result<String, UploadError> {
    let mut form = Form::new();
    if let Some(album_id) = picgo_album_id {
        form = form.text("album_id", album_id.to_string());
    }

    form = form.file("source", path).await?;

    let response = client
        .post(PICGO_HOST)
        .header("X-API-Key", picgo_api_key)
        .multipart(form)
        .send()
        .await?;

    let res: PicgoResult = response.json().await?;
    if res.status_code != 200 {
        let error = res
            .error
            .map(|e| e.message)
            .unwrap_or(String::from("api error"));
        warn!(error, "api error");
        return Err(UploadError::Api(error));
    }

    Ok(res.image.expect("must exist when 200").url)
}

async fn upload_postimage(client: &Client, path: impl AsRef<Path>) -> Result<String, UploadError> {
    let now = Timestamp::now().as_millisecond();
    let mut rng = rand::rng();
    let random_digits: String = (0..16)
        .map(|_| rng.random_range(0..10).to_string())
        .collect();
    let session = format!("{}.{}", now, random_digits);

    let form = Form::new()
        .text("gallery", "")
        .text("optsize", "0")
        .text("expire", "0")
        .text("numfiles", "1")
        .text("upload_session", session)
        .file("file", path)
        .await?;
    let response = client.post(POSTIMAGE_HOST).multipart(form).send().await?;

    let res: serde_json::Value = response.json().await?;
    dbg!(&res);
    let res: PostimageResult = serde_json::from_value(res)?;
    if let Some(error) = res.error {
        return Err(UploadError::Api(error.message));
    }

    let url = res.url.expect("must exist when no error");
    let response = client.get(url).send().await?.error_for_status()?;
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

    if let Some(start) = text.find(OG_IMAGE_PREFIX) {
        let content_start = start + OG_IMAGE_PREFIX.len();

        if let Some(end) = text[content_start..].find('"') {
            return Ok(text[content_start..content_start + end].to_string());
        }
    }

    Err(UploadError::Api(String::from("解析失败")))
}
