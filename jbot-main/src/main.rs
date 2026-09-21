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
use tokio::time::interval;

pub use crate::core::curl;
pub use crate::core::ffmpeg::{self, FFmpeg};
pub use crate::core::progress;
pub use crate::jbot_macro::{
    on_grouped_messages, on_interval, on_new_message, on_setup, on_update,
};

const IS_DEBUG: bool = cfg!(debug_assertions);

const SYNC_INTERVAL: Duration = Duration::from_secs(60);
const MAX_SYNC_INTERVAL: Duration = Duration::from_secs(600);
const SESSION_FILE: &str = "jbot.session";

async fn async_main() {
    for setup in core::update::SETUPS {
        if let Err(e) = setup() {
            log::error!("初始化失败: {e}");
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
        log::info!("Signing in...");
        client
            .bot_sign_in(&token, &api_hash)
            .await
            .expect("Sign in failed.");
        log::info!("Signed in!");
    }

    log::info!("Waiting for messages...");

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
    let mut timer = interval(Duration::from_secs(1));
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
                        log::error!("获取更新失败: {e}");
                    }
                }
            }
            Some(res) = handler_tasks.join_next(), if !handler_tasks.is_empty() => {
                if let Err(e) = res {
                    log::error!("handler task panicked: {e}");
                }
            }
            _ = sync_timer.tick() => {
                if dirty && (handler_tasks.is_empty() || last_save_time.elapsed() > MAX_SYNC_INTERVAL) {
                    log::info!("Saving session periodically...");
                    match updates
                        .sync_update_state()
                        .await {
                        Ok(_) => {
                            dirty = false;
                            last_save_time = Instant::now();
                        }
                        Err(e) => {
                            log::error!("Sync update state failed: {e}");
                        }
                    }
                }
            }
            _ = timer.tick() => {
                for handler in core::update::INTERVAL_HANDLERS {
                    handler_tasks.spawn(handler(client.clone(), Arc::clone(&session)));
                }
            }
        }
    }

    log::info!("Saving session file...");
    if let Err(e) = updates.sync_update_state().await {
        log::error!("Sync update state failed: {e}")
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
    log::info!("Gracefully closing connection to notify all pending handlers...");
    handle.quit();
    let _ = pool_task.await;

    // Give a chance to all on-going handlers to finish.
    // log::info!("Waiting for any slow handlers to finish...");
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
