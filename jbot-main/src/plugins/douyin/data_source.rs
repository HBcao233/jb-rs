use std::path::Path;

use serde_json::Value;
use tokio::fs;
use tracing::{error, info};
use wreq::{Client, StatusCode};

use super::abogus::{abogus, websign};
use super::types::{AwemeDetail, AwemeResult, GetAwemeError};

const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/137.0.0.0 Safari/537.36";
const COOKIE: &str = "SEARCH_RESULT_LIST_TYPE=%22single%22; hevc_supported=true; odin_tt=55dfb5fc5e18cbe6c9ae1cde4fcea19030bd5cf2f6878017dfec590258aad0daf46ee186814f3462c96235ce93024828033b43bbd5dd3d2cd73d4be13f790dd6c38ad0abf1e19023901cf8f4d1077dd6; enter_pc_once=1; UIFID=4b5df8f2f1e37245005fd22980d950d15862f006c9917e4d56af34274e9d091069d16eb83533042983a5affc457fe8f67a16d2fcfc0989a420ac1cddee10b3d302489db181a6aba66248d26970a0dcbb8b9a1d4c610eed0a10c334f5dc1c9df4042146f15c0b93f43e06f172128eb12c5b9428e9462de86b3bd4ef8bdf22baeab16471327145507e7abc28f2a0b277845df7be865f67092c4187af60e4635c06; volume_info=%7B%22isUserMute%22%3Afalse%2C%22isMute%22%3Afalse%2C%22volume%22%3A0.5%7D; home_can_add_dy_2_desktop=%220%22; is_dash_user=1; passport_csrf_token=18828a19eea858005294ed9086ff72f1; passport_csrf_token_default=18828a19eea858005294ed9086ff72f1; __security_mc_1_s_sdk_crypt_sdk=d0eaa373-4923-8cff; bd_ticket_guard_client_data=eyJiZC10aWNrZXQtZ3VhcmQtdmVyc2lvbiI6MiwiYmQtdGlja2V0LWd1YXJkLWl0ZXJhdGlvbi12ZXJzaW9uIjoxLCJiZC10aWNrZXQtZ3VhcmQtcmVlLXB1YmxpYy1rZXkiOiJCTFpjUzJYN1A0RXZreXlUOHFlSVQwZUhTVTlUUURBL3pVUm50RmdjM0NZdkwxMkk4b0JMOVN0V0VrQkJSdjlkZ0NsVUZLN1hRb0lnNjRmNDE1Qk5nQ1U9IiwiYmQtdGlja2V0LWd1YXJkLXdlYi12ZXJzaW9uIjoyfQ%3D%3D; bd_ticket_guard_client_web_domain=2; SEARCH_UN_LOGIN_PV_CURR_DAY=%7B%22date%22%3A1789271953167%2C%22count%22%3A3%7D; is_support_rtm_web_ts=1; strategyABtestKey=%221789439529.393%22; ttwid=1%7C8x4bRO_NTtYfuR6Zhia0uBJSO85gar9GtvpJkqGBIrA%7C1789439529%7C6349f3019e24b389ef89561fb75b7f08a55882ae01e74a4e5ad8ca6dc3cd8968; biz_trace_id=2fcfd313; IsDouyinActive=false; stream_recommend_feed_params=%22%7B%5C%22cookie_enabled%5C%22%3Atrue%2C%5C%22screen_width%5C%22%3A360%2C%5C%22screen_height%5C%22%3A800%2C%5C%22browser_online%5C%22%3Atrue%2C%5C%22cpu_core_num%5C%22%3A8%2C%5C%22device_memory%5C%22%3A8%2C%5C%22downlink%5C%22%3A10%2C%5C%22effective_type%5C%22%3A%5C%224g%5C%22%2C%5C%22round_trip_time%5C%22%3A50%7D%22";
const EMPTY_TITLE: &str = "&lt;空标题&gt;";

const AWEME_HOST: &str = "https://www-hj.douyin.com/aweme/v1/web/aweme/detail/";

pub async fn get_aweme_detail(client: &Client, aid: &str) -> Result<AwemeDetail, GetAwemeError> {
    let cache_dir = Path::new("cache/douyin");
    if let Err(e) = fs::create_dir_all(cache_dir).await {
        error!("缓存文件夹创建失败: {e:?}");
    }

    let cache_file = cache_dir.join(&format!("{aid}.json"));
    let res: Value = if let Ok(text) = fs::read_to_string(&cache_file).await {
        info!("使用缓存: {}", cache_file.display());
        serde_json::from_str(&text)?
    } else {
        let mut query = vec![
            ("aweme_id", aid.to_string()),
            ("device_platform", "webapp".to_string()),
            ("aid", "6383".to_string()),
            ("channel", "channel_pc_web".to_string()),
            ("request_source", "600".to_string()),
            ("origin_type", "video_page".to_string()),
            ("update_version_code", "170400".to_string()),
            ("pc_client_type", "1".to_string()),
            ("pc_libra_divert", "Linux".to_string()),
            ("support_h265", "1".to_string()),
            ("support_dash", "1".to_string()),
            ("cpu_core_num", "8".to_string()),
            ("version_code", "190500".to_string()),
            ("version_name", "19.5.0".to_string()),
            ("cookie_enabled", "true".to_string()),
            ("screen_width", "360".to_string()),
            ("screen_height", "800".to_string()),
            ("browser_language", "zh-CN".to_string()),
            ("browser_platform", "Linux armv81".to_string()),
            ("browser_name", "Chrome".to_string()),
            ("browser_version", "137.0.0.0".to_string()),
            ("browser_online", "true".to_string()),
            ("engine_name", "Blink".to_string()),
            ("engine_version", "137.0.0.0".to_string()),
            ("os_name", "Linux".to_string()),
            ("os_version", "x86_64".to_string()),
            ("device_memory", "8".to_string()),
            ("platform", "PC".to_string()),
            ("downlink", "10".to_string()),
            ("effective_type", "4g".to_string()),
            ("round_trip_time", "50".to_string()),
            ("webid", "7671197164462655017".to_string()),
            // ("uifid", UIFID.to_string()),
            (
                "verifyFp",
                "verify_msinqp0c_NoMtCO71_mqfy_4Bh8_AAln_PnDQzGIPPtcb".to_string(),
            ),
            (
                "fp",
                "verify_msinqp0c_NoMtCO71_mqfy_4Bh8_AAln_PnDQzGIPPtcb".to_string(),
            ),
            // ("msToken", "_9minsurW04nyIbLhfa1zAqAQRR84EjAXfaQ-p51krUAu6y5Ulgte5fbkUGwynzfRELviaEGQvIUf2uSADk7Au9j__y9gZvL9_94Zjacl6r2LEx6EE_VdFcoS9fgCcnf1rO1pgd04xQF0hXBBc2q6gdQ9ILsXomkdrnGlAvScg66L-PncTQ0".to_string()),
            // ("x-secsdk-web-signature", "c5f4be4a371e44e6861e914526967c20".to_string()),
        ];
        abogus(&mut query, "", Some(USER_AGENT), None);
        websign(&mut query);

        let response = client
            .get(AWEME_HOST)
            .header("user-agent", USER_AGENT)
            .header("cookie", COOKIE)
            .query(&query)
            .send()
            .await?;
        let status = response.status();
        if status != StatusCode::OK {
            info!("{:?}", response.text().await);
            return Err(GetAwemeError::Status(status.as_u16()));
        }

        let res: Value = response.json().await?;

        let pretty = serde_json::to_string_pretty(&res)?;
        if let Err(e) = fs::write(&cache_file, &pretty).await {
            error!("缓存json文件失败: {e:?}");
        } else {
            info!("写入缓存: {}", cache_file.display());
        }
        res
    };

    let result: AwemeResult = serde_json::from_value(res)?;
    if result.status_code != 0 {
        return Err(GetAwemeError::Api(result.status_code.to_string()));
    }
    match result.aweme_detail {
        Some(aweme_detail) => Ok(aweme_detail),
        None => Err(GetAwemeError::NotFound),
    }
}

pub fn parse_msg(res: &AwemeDetail) -> String {
    let aid = &res.aweme_id;
    let mut title = if res.item_title.is_empty() {
        res.desc.as_str()
    } else {
        res.item_title.as_str()
    };
    if title.is_empty() {
        title = EMPTY_TITLE;
    }

    let nickname = &res.author.nickname;
    let sec_uid = &res.author.sec_uid;
    format!(
        "<a href=\"https://www.douyin.com/video/{aid}\">{title}</a> - <a href=\"https://www.douyin.com/user/{sec_uid}\">{nickname}</a> #douyin"
    )
}
