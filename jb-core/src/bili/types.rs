//! types

use serde::Deserialize;
use wreq::header::InvalidHeaderValue;

use super::abv::{av2bv, bv2av};

pub(super) const QN: u8 = 80;

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
    #[error("请求失败")]
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
    pub(super) data: Option<serde_json::Value>,
    pub(super) message: String,
}

#[derive(Deserialize)]
pub(super) struct BiliResult1 {
    pub(super) code: i32,
    pub(super) result: Option<serde_json::Value>,
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
    /// 统计信息
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
    pub first_frame: Option<String>,
    /// 序号, 从 1 开始
    pub page: u16,
    /// 分集标题
    pub part: String,
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
    /// 时长, 秒
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

/// 番剧信息
#[derive(Deserialize)]
pub struct BangumiInfo {
    /// 演员
    pub actors: String,
    /// 制作人员名单
    pub staff: String,
    /// 区域
    pub areas: Vec<Area>,
    /// 封面 url
    pub cover: String,
    /// 网页端背景图url
    pub bkg_cover: String,
    /// 标题
    pub title: String,
    /// 子标题
    pub subtitle: String,
    /// 日文标题
    pub jp_title: String,
    /// 简介
    pub evaluate: String,
    /// 剧集类型: 1：番剧, 2：电影, 3：纪录片, 4：国创, 5：电视剧, 7：综艺
    pub r#type: i32,
    /// 番剧 id
    pub media_id: u64,
    /// 番剧介绍页链接: `http://www.bilibili.com/bangumi/media/md{media_id}/`
    pub link: String,
    /// 总计集数, 未完结大多为 -1 或 0
    pub total: i32,
    /// 标签
    pub styles: Vec<String>,
    /// 发布信息
    pub publish: DangumiPublish,
    /// 评分
    pub rating: DangumiRating,
    /// 统计信息
    pub stat: DangumiStat,
    /// 权限
    pub rights: Rights,
    /// 季 id
    pub season_id: u64,
    /// 季标题
    pub season_title: String,
    /// 剧集更新信息
    pub new_ep: DangumiNewEp,
    /// 系列信息
    pub series: DangumiSeries,
    /// 分享复制信息, 一般格式为 `《{series_title}》{share_sub_title}`
    pub share_copy: String,
    /// 分享备注
    pub share_sub_title: String,
    /// 分享链接
    pub share_url: String,
    /// up主 信息
    pub up_info: Option<UpInfo>,
    /// 季列表
    pub seasons: Vec<Season>,
    /// 剧集列表
    pub episodes: Vec<Episode>,
    /// 板块列表
    pub section: Vec<Section>,
}

/// 区域
///
/// | ID | 名称    |
/// |----|--------|
/// | 1  | 中国大陆  |
/// | 2  | 日本     |
/// | 3  | 美国     |
/// | 4  | 英国     |
/// | 5  | 加拿大    |
/// | 6  | 中国香港  |
/// | 7  | 中国台湾  |
/// | 8  | 韩国     |
/// | 9  | 法国     |
/// | 10 | 泰国     |
/// | 11 | 马来西亚  |
/// | 12 | 新加坡   |
/// | 13 | 西班牙   |
/// | 14 | 俄罗斯   |
/// | 15 | 德国    |
/// | 16 | 其他    |
/// | 17 | 丹麦    |
/// | 18 | 乌克兰   |
/// | 19 | 以色列   |
/// | 20 | 伊朗    |
/// | 21 | 保加利亚  |
/// | 22 | 克罗地亚  |
/// | 23 | 冰岛    |
/// | 24 | 匈牙利   |
/// | 25 | 南非    |
/// | 26 | 印尼    |
/// | 27 | 印度    |
/// | 28 | 哥伦比亚  |
/// | 30 | 土耳其   |
/// | 31 | 墨西哥   |
/// | 32 | 委内瑞拉  |
/// | 33 | 巴西    |
/// | 34 | 希腊    |
/// | 35 | 意大利   |
/// | 36 | 挪威    |
/// | 37 | 捷克    |
/// | 38 | 摩洛哥   |
/// | 39 | 新西兰   |
/// | 40 | 智利    |
/// | 41 | 比利时   |
/// | 42 | 波兰    |
/// | 43 | 澳大利亚  |
/// | 44 | 爱尔兰   |
/// | 45 | 瑞典    |
/// | 46 | 瑞士    |
/// | 47 | 芬兰    |
/// | 48 | 苏联    |
/// | 49 | 荷兰    |
/// | 50 | 越南    |
/// | 51 | 阿根廷   |
/// | 52 | 马耳他   |
/// | 53 | 古巴    |
/// | 54 | 菲律宾   |
/// | 55 | 哈萨克斯坦 |
/// | 56 | 黎巴嫩   |
/// | 57 | 塞浦路斯  |
/// | 58 | 卡塔尔   |
/// | 59 | 阿联酋   |
/// | 60 | 奥地利   |
/// | 61 | 西德    |
/// | 62 | 卢森堡   |
/// | 63 | 罗马尼亚  |
/// | 64 | 印度尼西亚 |
/// | 65 | 南斯拉夫  |
/// | 66 | 蒙古    |
/// | 70 | 葡萄牙   |
#[derive(Deserialize)]
pub struct Area {
    /// 区域 id
    pub id: i32,
    /// 区域名称
    pub name: String,
}

/// 剧集更新信息
#[derive(Deserialize)]
pub struct DangumiNewEp {
    /// 更新备注, 例如 "每周五12:00更新1集，每周二12:00更新下集预告"
    pub desc: String,
    /// 最新一话剧集 ep_id
    pub id: u64,
    /// 是否最新发布, 0: 否, 1: 是
    pub is_new: i32,
    /// 最新一话标题
    pub title: String,
}

/// 发布信息
#[derive(Deserialize)]
pub struct DangumiPublish {
    /// 是否完结, 0: 否, 1: 是
    pub is_finish: i32,
    /// 是否开播, 0: 否, 1: 是
    pub is_started: i32,
    /// 开播日期, 一般格式为 `yyyy-MM-dd HH::mm::ss`
    pub pub_time: String,
    /// 网页端展示用开播日期, 一般格式为 `yyyy年MM月dd日HH::mm`
    pub pub_time_show: String,
    /// 是否未知开播日期, 0: 否, 1: 是
    pub unknow_pub_date: i32,
    /// 作用尚不明确
    pub weekday: i32,
}

/// 评分信息
#[derive(Deserialize)]
pub struct DangumiRating {
    /// 评分人数
    pub count: u32,
    /// 分数
    pub score: f32,
}

/// 系列信息
#[derive(Deserialize)]
pub struct DangumiSeries {
    /// 暂时不知道作用
    pub display_type: i32,
    /// 系列 id
    pub series_id: u32,
    /// 系列标题
    pub series_title: String,
}

/// 统计信息
#[derive(Deserialize)]
pub struct DangumiStat {
    /// 投币
    pub coins: u32,
    /// 弹幕
    pub danmakus: u32,
    /// 追番
    pub favorite: u32,
    /// 系列总追番数
    pub favorites: u32,
    /// 追番文本
    pub follow_text: String,
    /// 点赞
    pub likes: u32,
    /// 评论
    pub reply: u32,
    /// 分享
    pub share: u32,
    /// 播放量
    pub views: u64,
    // 暂时不知道作用，固定为 0
    // pub vt: i32,
}

/// Uploader 信息
#[derive(Deserialize)]
pub struct UpInfo {
    /// 用户id
    pub mid: u32,
    /// 头像图片链接
    pub avatar: String,
    /// 名称
    pub uname: String,
}

/// 权限信息
#[derive(Deserialize)]
pub struct Rights {
    /// 暂时不知道含义, 0: 否, 1: 是
    pub allow_bp: i32,
    /// 暂时不知道含义, 0: 否, 1: 是
    pub allow_bp_rank: i32,
    /// 是否允许下载, 0: 否, 1: 是
    pub allow_download: i32,
    /// 是否允许评论, 0: 否, 1: 是
    pub allow_review: i32,
    /// 区域限制, 暂时不知具体数值对应关系
    pub area_limit: i32,
    /// 禁播地区显示, 0: 否, 1: 是
    pub ban_area_show: i32,
    /// 是否可以观看
    pub can_watch: i32,
    /// 版权信息, bilibili: 授权, dujia: 独家
    pub copyright: String,
    // pub forbid_pre: i32,
    // pub freya_white: i32,
    // pub is_cover_show: i32,
    // pub is_preview: i32,
    // pub is_sponsor: i32,
    // pub only_vip_download: i32,
    // pub resource: String,
    // pub watch_platform: i32,
}

/// 季
#[derive(Deserialize)]
pub struct Season {
    /// 可能的值: "授权" / "独家" / "出品"
    pub badge: String,
    /// 封面图片链接
    pub cover: String,
    /// 横版封面 16:10
    pub horizontal_cover_1610: String,
    /// 横版封面 16:9
    pub horizontal_cover_169: String,
    /// 番剧 id
    pub media_id: u32,
    /// 更新信息
    pub new_ep: SeasonNewEp,
    /// 季id
    pub season_id: u32,
    /// 季标题
    pub season_title: String,
    /// 季类型
    pub season_type: i32,
    /// 统计信息
    pub stat: SeasonStat,
}

/// 季更新信息
#[derive(Deserialize)]
pub struct SeasonNewEp {
    /// 最新一集封面
    pub cover: String,
    /// 最新一集 ep_id
    pub id: u64,
    /// 最新一集展示文本
    pub index_show: String,
}

/// 季统计信息
#[derive(Deserialize)]
pub struct SeasonStat {
    /// 追番
    pub favorites: u32,
    /// 系列总追番数
    pub series_follow: u32,
    /// 播放量
    pub views: u64,
    // vt: 0
}

/// 板块
#[derive(Deserialize)]
pub struct Section {
    /// 板块 id
    pub id: u64,
    /// 板块类型, 1、2: 普通板块, 5: 其他UP视频收录板块
    pub r#type: i32,
    /// 板块标题, 一般为 "花絮/预告"
    pub title: String,
    /// 板块内容
    pub episodes: Vec<SectionEpisode>,
}

/// 剧集信息
#[derive(Deserialize)]
pub struct Episode {
    /// aid
    pub aid: u64,
    /// bvid
    pub bvid: String,
    /// cid
    pub cid: i64,
    /// 可能的值: "" / "会员" / "限免" / "预告"
    pub badge: String,
    /// 封面
    pub cover: String,
    /// 分辨率
    pub dimension: Dimension,
    /// 时长, 秒
    pub duration: u32,
    /// ep_id
    pub ep_id: u64,
    // 暂时不知道作用
    // pub vid: Option<String>,
    // 可能的值: "bangumi"
    // pub from: Option<String>,
    /// 播放链接, 格式: `https://www.bilibili.com/bangumi/play/ep{ep_id}`
    pub link: String,
    /// 短链, 格式 `https://b23.tv/ep{ep_id}`
    pub short_link: String,
    /// 发布日期
    pub pub_time: u64,
    // 暂时不知道含义, 固定为 0
    // pub pv: i32,
    /// 是否是花絮/预告, 0: 正片, 1: 花絮/预告
    pub section_type: i32,
    /// 标题
    pub title: String,
    /// 副标题
    pub subtitle: String,
    /// 长标题
    pub long_title: String,
    /// 显示标题
    pub show_title: String,
    /// 分享文本, 格式 `《{season_title}》{show_title}`
    pub share_copy: String,
    /// 分享链接, 同 link
    pub share_url: String,
    /// 跳过片头片尾信息
    pub skip: EpisodeSkip,
}

/// 板块剧集信息
#[derive(Deserialize)]
pub struct SectionEpisode {
    /// aid
    pub aid: u64,
    /// bvid
    pub bvid: Option<String>,
    /// cid
    pub cid: i64,
    /// 可能的值: "" / "会员" / "限免" / "预告"
    pub badge: String,
    /// 封面
    pub cover: String,
    /// 分辨率
    pub dimension: Option<Dimension>,
    /// 时长, 秒
    pub duration: Option<u32>,
    /// ep_id
    pub ep_id: u64,
    // 暂时不知道作用
    // pub vid: Option<String>,
    // 可能的值: "bangumi"
    // pub from: Option<String>,
    /// 播放链接, 格式: `https://www.bilibili.com/bangumi/play/ep{ep_id}`
    pub link: String,
    /// 短链, 格式 `https://b23.tv/ep{ep_id}`
    pub short_link: Option<String>,
    /// 发布日期
    pub pub_time: u64,
    // 暂时不知道含义, 固定为 0
    // pub pv: i32,
    /// 是否是花絮/预告, 0: 正片, 1: 花絮/预告
    pub section_type: i32,
    /// 标题
    pub title: String,
    /// 副标题
    pub subtitle: Option<String>,
    /// 长标题
    pub long_title: Option<String>,
    /// 显示标题
    pub show_title: Option<String>,
    /// 分享文本, 格式 `《{season_title}》{show_title}`
    pub share_copy: Option<String>,
    /// 分享链接, 同 link
    pub share_url: Option<String>,
    /// 跳过片头片尾信息
    pub skip: Option<EpisodeSkip>,
    /// 统计信息
    pub stat: Option<EpisodeStat>,
    /// link_type, 可能的值: None, Some("BVID")
    pub link_type: Option<String>,
    // link_type 为 "BVID" 时存在
    //pub report: Option<EpisodeReport>,
}

/// 跳过片头片尾信息
#[derive(Deserialize)]
pub struct EpisodeSkip {
    /// 片头
    pub op: Skip,
    /// 片尾
    pub ed: Skip,
}

/// 跳过信息
#[derive(Deserialize)]
pub struct Skip {
    /// 开始
    pub start: u32,
    /// 结束
    pub end: u32,
}

/// 剧集权限
#[derive(Deserialize)]
pub struct EpisodeRights {
    /// 允许弹幕, 0: 否, 1: 是
    pub allow_dm: i32,
    /// 允许下载, 0: 否, 1: 是
    pub allow_download: i32,
    /// 地区限制, 0: 无限制
    pub area_limit: i32,
    // 暂时不知道含义
    // pub cache_auth: i32,
}

/// 剧集统计信息
#[derive(Deserialize)]
pub struct EpisodeStat {
    /// 投币
    pub coin: u32,
    /// 弹幕
    pub danmakus: u32,
    /// 点赞
    pub likes: u32,
    /// 播放量
    pub play: u64,
    /// 评论
    pub reply: u32,
    // vt: 0
}

/*
/// report
#[derive(Deserialize)]
pub struct EpisodeReport {
    pub aid: String,
    pub ep_title: String,
    pub season_id: String,
    pub season_type: String,
    pub section_id: String,
    pub section_type: String,
}
*/
