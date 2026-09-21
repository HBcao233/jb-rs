extern crate jbot_macro;
mod core;
pub mod database;
mod plugins;
pub mod utils;

use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use grammers_client::Client;
use grammers_client::sender::{SenderPool, UpdatesConfiguration};
use grammers_session::storages::SqliteSession;
use tokio::runtime;
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval};
use tracing::{error, info};

#[cfg(feature = "core_curl")]
pub use crate::core::curl;
pub use crate::core::ffmpeg::{self, FFmpeg};
pub use crate::core::progress;
pub use crate::jbot_macro::{
    on_grouped_messages, on_interval, on_new_message, on_setup, on_update,
};

// debug 模式不 catch_up 追赶更新
const IS_DEBUG: bool = cfg!(debug_assertions);

// 定时任务间隔
const INTERVAL_TIME: Duration = Duration::from_secs(1);

// 同步 session 相关
const SYNC_INTERVAL: Duration = Duration::from_secs(60);
const MAX_SYNC_INTERVAL: Duration = Duration::from_secs(600);
const SESSION_FILE: &str = "jbot.session";

async fn async_main() {
    for setup in core::update::SETUPS {
        if let Err(e) = setup() {
            error!("初始化失败: {e}");
            return;
        }
    }

    let api_id = env::var("TG_ID")
        .unwrap_or_default()
        .parse()
        .expect("TG_ID invalid");
    let api_hash = env::var("TG_HASH").unwrap();
    let token = env::var("TOKEN").expect("token missing");

    let session = Arc::new(SqliteSession::open(SESSION_FILE).await.unwrap());

    let SenderPool {
        runner,
        updates,
        handle,
    } = SenderPool::new(Arc::clone(&session), api_id);
    let client = Client::new(handle.clone());
    let pool_task = tokio::spawn(runner.run());

    if !client.is_authorized().await.unwrap() {
        info!("Signing in...");
        client
            .bot_sign_in(&token, &api_hash)
            .await
            .expect("Sign in failed.");
        info!("Signed in!");
    }

    let bg_client = client.clone();
    let bg_session = Arc::clone(&session);
    tokio::spawn(async move {
        let mut interval = interval(INTERVAL_TIME);
        // 忽略错过的 tick
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        // 跳过第一次
        interval.tick().await;
        loop {
            interval.tick().await;
            for handler in core::update::INTERVAL_HANDLERS {
                handler(bg_client.clone(), Arc::clone(&bg_session)).await;
            }
        }
    });

    info!("Waiting for messages...");

    let mut handler_tasks = JoinSet::new();
    let mut updates = client
        .stream_updates(
            updates,
            UpdatesConfiguration {
                catch_up: !IS_DEBUG,
            },
        )
        .await
        .unwrap();

    let mut sync_timer = interval(SYNC_INTERVAL);
    let mut dirty = false;
    let mut last_save_time = Instant::now();

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            result = updates.next() => {
                dirty = true;
                match result {
                    Ok(update) => {
                        let handle = client.clone();
                        handler_tasks.spawn(core::update::handle_update(handle, update, Arc::clone(&session)));
                    }
                    Err(e) => {
                        error!("获取更新失败: {e}");
                    }
                }
            }
            Some(res) = handler_tasks.join_next(), if !handler_tasks.is_empty() => {
                if let Err(e) = res {
                    error!("handler task panicked: {e}");
                }
            }
            _ = sync_timer.tick() => {
                if dirty && (handler_tasks.is_empty() || last_save_time.elapsed() > MAX_SYNC_INTERVAL) {
                    info!("Saving session periodically...");
                    match updates
                        .sync_update_state()
                        .await {
                        Ok(_) => {
                            dirty = false;
                            last_save_time = Instant::now();
                        }
                        Err(e) => {
                            error!("Sync update state failed: {e}");
                        }
                    }
                }
            }
        }
    }

    info!("Saving session file...");
    if let Err(e) = updates.sync_update_state().await {
        error!("Sync update state failed: {e}")
    }

    // Pool's `run()` won't finish until all handles are dropped or quit is called.
    // Here there are at least three handles alive: `handle`, `client` and `updates`
    // which contains a `client`. Any ongoing `handle_update` handlers have one client too.
    // In this case, it's easier to call `handle.quit()` to close them all.
    //
    // You don't need to explicitly close the connection, but this is a way to do it gracefully.
    // This also gives a chance to the handlers to finish their work by handling the `Dropped`
    // error from any pending method calls (RPC invocations).
    //
    // You can try this graceful shutdown by sending a message saying "slow" and then pressing Ctrl+C.
    info!("Gracefully closing connection to notify all pending handlers...");
    handle.quit();
    let _ = pool_task.await;

    // Give a chance to all on-going handlers to finish.
    // info!("Waiting for any slow handlers to finish...");
    // while let Some(_) = handler_tasks.join_next().await {}
}

fn main() {
    dotenvy::dotenv().unwrap();

    let guard = core::log::init_log().unwrap();

    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main());

    drop(guard);
}
