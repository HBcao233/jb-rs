use wreq::header;
use serde::Deserialize;

const CLIENT_NAME: &str = "1";
const CLIENT_VERSION: &str = "2.20260708.00.00";

#[derive(Deserialize, Default, Debug)]
struct Ytcfg {
    VISITOR_DATA: String,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = wreq::Client::builder().emulation(wreq_util::Emulation::Chrome137).build()?;
    let response = client
        .get("https://www.youtube.com/")
        .header("X-YouTube-Client-Name", CLIENT_NAME)
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .header(header::ORIGIN, "www.youtube.com")
        .send()
        .await?;
    let text = response.text().await?;
    // tokio::fs::write(std::path::Path::new("a.html"), &text).await;
    let ytcfg = if let Some(pos1) = text.find("ytcfg.set({") && let Some(pos2) = text.find("}); "){
        match text.get((pos1 + 10)..(pos2 + 1)) {
            Some(cfg) => serde_json::from_str(cfg).unwrap_or_default(),
            None => Ytcfg::default(),
        }
    } else {
        Ytcfg::default()
    };
    dbg!(ytcfg);
    Ok(())
}
