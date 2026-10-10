//! types

use serde::Deserialize;
use wreq::header::InvalidHeaderValue;

use super::abv::{av2bv, bv2av};

pub(super) const QN: u8 = 80;

pub(super) const NAV_HOST: &str = "https://api.bilibili.com/x/web-interface/nav";
pub(super) const INFO_HOST: &str = "https://api.bilibili.com/x/web-interface/wbi/view/detail";
pub(super) const PLAYURL_HOST: &str = "https://api.bilibili.com/x/player/wbi/playurl";
pub(super) const FINGER_HOST: &str = "https://api.bilibili.com/x/frontend/finger/spi";

pub(super) const GAIA_VGATE_HOST: &str = "https://api.bilibili.com/x/gaia-vgate/v1/register";
pub(super) const GAIA_VALIDATE_HOST: &str = "https://api.bilibili.com/x/gaia-vgate/v1/validate";

pub(super) const MIXIN_KEY_ENC_TAB: [u8; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29,
    28, 14, 39, 12, 38, 41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25,
    54, 21, 56, 59, 6, 63, 57, 62, 11, 36, 20, 34, 44, 52,
];

#[derive(Deserialize)]
pub(super) struct FingerResult {
    pub(super) data: FingerData,
}

#[derive(Deserialize)]
pub(super) struct FingerData {
    pub(super) b_3: String,
    pub(super) b_4: String,
}

#[derive(Deserialize)]
pub(super) struct NavResult {
    pub(super) data: NavData,
}

#[derive(Deserialize)]
pub(super) struct NavData {
    pub(super) wbi_img: WbiImg,
}

#[derive(Deserialize)]
pub(super) struct WbiImg {
    pub(super) img_url: String,
    pub(super) sub_url: String,
}

/// av2bv bv2av 包装
#[derive(Clone)]
pub enum BiliId {
    /// avid
    AV(u64),
    /// bvid 以 BV1 开头
    BV(String),
}

impl BiliId {
    /// 获取 aid, bvid
    pub fn to_raw(&self) -> Option<(u64, String)> {
        match self {
            Self::AV(aid) => Some((*aid, av2bv(*aid)?)),
            Self::BV(bvid) => Some((bv2av(bvid)?, bvid.to_string())),
        }
    }
}

/// Bilibili 请求错误
#[derive(Debug, thiserror::Error)]
pub enum BiliError {
    /// 无效的请求头
    #[error("无效的请求头: {0}")]
    InvalidHeaderValue(#[from] InvalidHeaderValue),

    /// io error
    #[error("IO 错误: {0}")]
    Io(#[from] tokio::io::Error),

    /// http error
    #[error("请求失败: {0}")]
    Http(#[from] wreq::Error),

    /// StatusCode is not 200
    #[error("状态码错误: {0}")]
    Status(u16),

    /// json error
    #[error("JSON 解析失败: {0}")]
    Json(#[from] serde_json::Error),

    /// bilibili video not found
    #[error("Bili不存在")]
    NotFound,

    /// Api error
    #[error("API 错误: {0}")]
    Api(String),

    /// 风控
    #[error("触发哔哩哔哩安全风控策略，访问请求被拒绝。")]
    RiskControl,

    /// 需要人机验证
    #[error("需要人机验证")]
    Voucher(String),
}

#[derive(Deserialize)]
pub(super) struct BiliResult {
    pub(super) code: i32,
    pub(super) data: serde_json::Value,
    pub(super) message: String,
}

#[derive(Deserialize)]
pub(super) struct BiliDetail {
    #[serde(rename = "View")]
    pub(super) view: BiliInfo,
}

/// 视频信息
#[derive(Debug, Deserialize)]
pub struct BiliInfo {
    /// aid
    pub aid: i64,
    /// bvid
    pub bvid: String,
    /// P1 的 cid
    pub cid: i64,
    /// 创建日期
    pub ctime: i64,
    /// 发布日期
    pub pubdate: i64,
    /// 简介
    pub desc_v2: Option<Vec<DescItem>>,
    /// 分辨率
    pub dimension: Dimension,
    /// 时长, 单位: 秒
    pub duration: u32,
    /// 作者
    pub owner: Owner,
    /// 分P
    pub pages: Vec<Page>,
    /// 封面
    pub pic: String,
    /// 统计数据
    pub stat: Stat,
    /// 标题
    pub title: String,
}

/// 简介 v2 item
#[derive(Debug, Deserialize)]
pub struct DescItem {
    /// 提及 user id
    pub biz_id: u64,
    /// 原始文本
    pub raw_text: String,
    /// 类型. type == 2 时为提及用户
    pub r#type: u8,
}

/// 分辨率
#[derive(Debug, Deserialize)]
pub struct Dimension {
    /// 宽度
    pub width: u16,
    /// 高度
    pub height: u16,
}

/// 作者信息
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Owner {
    /// 头像链接
    pub face: String,
    /// id
    pub mid: u64,
    /// 名称
    pub name: String,
}

/// 分集信息
#[derive(Debug, Deserialize)]
pub struct Page {
    /// cid
    pub cid: i64,
    /// 创建日期
    pub ctime: i64,
    /// 分辨率
    pub dimension: Dimension,
    /// 时长
    pub duration: u32,
    /// 第一帧图片链接
    pub first_frame: String,
    /// 序号, 从 1 开始
    pub page: u16,
}

/// 统计信息
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Stat {
    /// 投币
    pub coin: u32,
    /// 弹幕
    pub danmaku: u32,
    /// 点踩
    pub dislike: u32,
    /// 收藏
    pub favorite: u32,
    /// 点赞
    pub like: u32,
    /// 评论
    pub reply: u32,
    /// 分享
    pub share: u32,
    /// 播放量
    pub view: u32,
}

/// 视频播放流信息
#[derive(Debug, Deserialize)]
pub struct PlayurlInfo {
    /// 当前 durl 画质id
    pub quality: Option<i32>,
    /// 所有可选的视频画质id
    pub accept_quality: Option<Vec<i32>>,
    /// 所以可选的视频画质名
    pub accept_description: Option<Vec<String>>,
    /// dash 流
    pub dash: Option<DashInfo>,
    /// durl 流
    pub durl: Option<Vec<DurlInfo>>,
}

/// dash 流信息
#[derive(Debug, Deserialize)]
pub struct DashInfo {
    /// 时长
    pub duration: u32,
    /// 音频
    pub audio: Option<Vec<DashMedia>>,
    /// 视频
    pub video: Vec<DashMedia>,
}

/// dash 音视频流信息
#[derive(Debug, Deserialize)]
pub struct DashMedia {
    /// 画质 id
    pub id: i32,
    /// 链接
    pub base_url: String,
    /// 备用链接
    pub backup_url: Vec<String>,
    // pub width: u16,
    // pub height: u16,
    /// mime type
    pub mime_type: String,
    /// 带宽, bandwidth * duration / 8 即文件大小
    pub bandwidth: u32,
    /// 编码 id
    pub codecid: i32,
    /// 编码
    pub codecs: String,
}

/// durl 流信息
#[derive(Debug, Deserialize)]
pub struct DurlInfo {
    /// 文件大小
    pub size: u32,
    /// 链接
    pub url: String,
    /// 备用链接
    pub backup_url: Option<Vec<String>>,
}

#[derive(Deserialize)]
pub(super) struct GaiaVgateResult {
    pub(super) data: VgateData,
}

/// 人机验证信息
#[derive(Deserialize)]
pub struct VgateData {
    /// 类型, 当前只可能为 1: geetest 极验
    pub r#type: String,
    /// token
    pub token: String,
    /// 极验信息
    pub geetest: Geetest,
}

/// 极验信息
#[derive(Deserialize)]
pub struct Geetest {
    /// challenge
    pub challenge: String,
    /// gt
    pub gt: String,
}

#[derive(Deserialize)]
pub(super) struct GaiaValidateResult {
    // pub(super) code: i32,
    pub(super) data: GaiaValidateData,
}

/// 验证信息
#[derive(Deserialize)]
pub struct GaiaValidateData {
    /// 为 1: 表示成功
    pub is_valid: i32,
    /// 验证交付
    pub grisk_id: String,
}
