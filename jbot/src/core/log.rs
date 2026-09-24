use std::sync::LazyLock;

use jiff::tz::TimeZone;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::fmt::{format::Writer, time::FormatTime};

static TZ: LazyLock<TimeZone> = LazyLock::new(|| {
    let tz = std::env::var("TZ");
    let tz = tz.as_deref().unwrap_or("Asia/Shanghai");
    TimeZone::get(tz).unwrap()
});

#[derive(Debug, Clone)]
struct Timer;

impl FormatTime for Timer {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        let zoned = jiff::Timestamp::now().to_zoned(TZ.clone());
        write!(w, "{}", zoned.strftime("%Y-%m-%d %H:%M:%S"))
    }
}

pub fn init_log() -> Result<WorkerGuard, Box<dyn std::error::Error + Send + Sync>> {
    let appender = tracing_appender::rolling::daily("logs", "jbot.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(appender);

    tracing_subscriber::fmt()
        .with_writer(non_blocking.and(std::io::stderr))
        .with_timer(Timer)
        .init();

    Ok(guard)
}
