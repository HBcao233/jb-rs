use std::ffi::{OsStr, OsString};
use std::io;
use std::process::{ExitStatus, Stdio};

use indicatif::{ProgressBar, ProgressStyle};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Debug, Default)]
pub struct FFmpeg {
    bin: OsString,
    args: Vec<OsString>,
}

impl FFmpeg {
    pub fn new() -> Self {
        Self {
            bin: OsString::from("ffmpeg"),
            args: Vec::new(),
        }
    }

    /// 添加单个参数
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// 批量添加参数（保留顺序）
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for a in args {
            self.args.push(a.as_ref().to_os_string());
        }
        self
    }

    /// 执行 FFmpeg，并通过回调报告进度 `(current_ms, total_ms)`
    /// `total_ms`：总时长（毫秒），无法获取时为 `None`
    pub async fn run(self) -> io::Result<ExitStatus> {
        let mut child = Command::new(&self.bin)
            .arg("-hide_banner")
            .arg("-progress")
            .arg("pipe:2")
            .arg("-nostats")
            .args(&self.args)
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;

        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io::Error::other("failed to capture ffmpeg stderr"))?;

        let mut lines = BufReader::new(stderr).lines();

        let mut total_ms: Option<usize> = None;
        let mut last_ms: Option<usize> = None;
        let bar = ProgressBar::no_length();

        bar.set_style(
            ProgressStyle::default_bar()
                .template(" [{wide_bar}] {len}/{pos} ({percent}%)")
                .unwrap()
                .progress_chars("=> "),
        );
        while let Some(line) = lines.next_line().await? {
            if total_ms.is_none() {
                if let Some(duration) = parse_duration_line(&line) {
                    total_ms = Some(duration);
                    bar.set_length(duration as u64);
                }
            }

            if let Some(current_ms) = parse_progress_time(&line)
                .or_else(|| parse_out_time_us(&line))
                .or_else(|| parse_stats_time(&line))
            {
                if last_ms != Some(current_ms) {
                    last_ms = Some(current_ms);
                    bar.set_position(current_ms as u64);
                }
            }
        }

        child.wait().await
    }
}

/// 解析：Duration: 00:01:23.45, start: ...
fn parse_duration_line(line: &str) -> Option<usize> {
    let (_, value) = line.split_once("Duration:")?;
    let value = value.trim_start().split(',').next()?.trim();

    if value == "N/A" {
        return None;
    }

    parse_timestamp_ms(value)
}

/// 解析：out_time=00:00:10.123456
fn parse_progress_time(line: &str) -> Option<usize> {
    let value = line.trim().strip_prefix("out_time=")?;
    parse_timestamp_ms(value)
}

/// 解析：out_time_us=10123456
fn parse_out_time_us(line: &str) -> Option<usize> {
    let value = line.trim().strip_prefix("out_time_us=")?;
    let microseconds = value.parse::<u128>().ok()?;
    usize::try_from(microseconds / 1_000).ok()
}

/// 兼容普通 FFmpeg 日志：
/// frame=... time=00:00:10.12 bitrate=...
fn parse_stats_time(line: &str) -> Option<usize> {
    let index = line.find("time=")?;
    let value = line[index + "time=".len()..].split_whitespace().next()?;

    parse_timestamp_ms(value)
}

/// 将 HH:MM:SS、HH:MM:SS.xx 或 HH:MM:SS.xxxxxx 转为毫秒。
fn parse_timestamp_ms(value: &str) -> Option<usize> {
    let value = value.trim();

    // 负时间戳通常出现在某些滤镜或编码器初始化阶段，将其视为 0。
    if value.starts_with('-') {
        return Some(0);
    }

    let mut parts = value.split(':');

    let hours = parts.next()?.parse::<u128>().ok()?;
    let minutes = parts.next()?.parse::<u128>().ok()?;
    let seconds_part = parts.next()?;

    if parts.next().is_some() {
        return None;
    }

    let (seconds, fraction) = match seconds_part.split_once('.') {
        Some((seconds, fraction)) => (seconds, fraction),
        None => (seconds_part, ""),
    };

    let seconds = seconds.parse::<u128>().ok()?;

    // 小数部分截断或补齐到 3 位毫秒。
    let mut millis_text = String::with_capacity(3);
    for ch in fraction.chars().take(3) {
        if !ch.is_ascii_digit() {
            break;
        }
        millis_text.push(ch);
    }

    while millis_text.len() < 3 {
        millis_text.push('0');
    }

    let millis = millis_text.parse::<u128>().ok()?;

    let total_ms = hours
        .checked_mul(3_600_000)?
        .checked_add(minutes.checked_mul(60_000)?)?
        .checked_add(seconds.checked_mul(1_000)?)?
        .checked_add(millis)?;

    usize::try_from(total_ms).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "expensive"]
    async fn test_ffmpeg() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let path = path.parent().unwrap();
        let status = FFmpeg::new()
            .overwrite()
            .input(path.join("target/input.mp4"))
            .output(path.join("target/output.mp4"))
            .run_with_progress(|current, total| {
                println!("{} / {:?}", current, total);
            })
            .await
            .unwrap();
        let _ = dbg!(status);
    }
}
