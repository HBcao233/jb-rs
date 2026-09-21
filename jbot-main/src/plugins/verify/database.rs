use std::sync::Arc;

use grammers_session::types::PeerId;
use jiff::Timestamp;
use libsql::{Builder, Connection, Error, named_params, params};
use tokio::fs;
use tokio::sync::OnceCell;
use tracing::error;

#[derive(Debug)]
pub enum VerifyStatus {
    Null,
    Verifying {
        date: Timestamp,
        message_id: i32,
        solution: u32,
    },
    Verified,
    Banned,
}

const VERSION: i64 = 1;

#[derive(Debug)]
struct Database(Connection);

impl Database {
    async fn open() -> libsql::Result<Self> {
        if let Err(e) = fs::create_dir_all("data").await {
            error!("数据文件夹创建失败: {e:?}");
            return Err(Error::ConnectionFailed("数据文件夹创建失败".to_string()));
        }
        let conn = Builder::new_local("data/verify.db")
            .build()
            .await?
            .connect()?;
        let db = Database(conn);
        db.init().await?;

        Ok(db)
    }

    async fn init(&self) -> libsql::Result<()> {
        let mut user_version: i64 = self
            .fetch_one("PRAGMA user_version", params![], |row| row.get(0))
            .await?
            .unwrap_or(0);
        if user_version == VERSION {
            return Ok(());
        }

        if user_version == 0 {
            self.migrate_v0_to_v1().await?;
            user_version += 1;
        }
        if user_version == VERSION {
            // Can't bind PRAGMA parameters, but `VERSION` is not user-controlled input.
            self.0
                .execute(&format!("PRAGMA user_version = {VERSION}"), params![])
                .await?;
        }
        Ok(())
    }

    async fn migrate_v0_to_v1(&self) -> libsql::Result<()> {
        let transaction = self.begin_transaction().await?;
        transaction
            .execute(
                "CREATE TABLE verify (
                id INTEGER PRIMARY KEY,
                peer_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                status INTEGER,
                date INTEGER,
                message_id INTEGER,
                solution INTEGER,
                UNIQUE(peer_id, user_id))",
                params![],
            )
            .await?;

        transaction.commit().await?;
        Ok(())
    }

    async fn begin_transaction(&self) -> libsql::Result<libsql::Transaction> {
        self.0.transaction().await
    }

    async fn fetch_one<
        T,
        P: libsql::params::IntoParams,
        F: FnOnce(libsql::Row) -> libsql::Result<T>,
    >(
        &self,
        statement: &str,
        params: P,
        select: F,
    ) -> libsql::Result<Option<T>> {
        let mut statement = self.0.prepare(statement).await?;
        let result = statement.query_row(params).await;
        match result {
            Ok(value) => Ok(Some(select(value)?)),
            Err(libsql::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn fetch_all<
        T,
        P: libsql::params::IntoParams,
        F: FnMut(libsql::Row) -> libsql::Result<T>,
    >(
        &self,
        statement: &str,
        params: P,
        mut select: F,
    ) -> libsql::Result<Vec<T>> {
        let statement = self.0.prepare(statement).await?;
        let mut rows = statement.query(params).await?;
        let mut result = Vec::new();
        while let Some(row) = rows.next().await? {
            result.push(select(row)?);
        }
        Ok(result)
    }
}

pub async fn get_status(peer_id: PeerId, user_id: PeerId) -> libsql::Result<VerifyStatus> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let user_id = user_id.bot_api_dialog_id().unwrap();

    let db = Database::open().await?;
    let map_row = |row: libsql::Row| {
        let status: Option<u32> = row.get(0)?;
        Ok(match status {
            Some(1) => VerifyStatus::Verifying {
                date: Timestamp::from_millisecond(row.get(1)?).unwrap(),
                message_id: row.get(2)?,
                solution: row.get(3)?,
            },
            Some(2) => VerifyStatus::Verified,
            Some(3) => VerifyStatus::Banned,
            _ => VerifyStatus::Null,
        })
    };

    match db
        .fetch_one(
            "SELECT status, date, message_id, solution FROM verify WHERE peer_id = :peer_id AND user_id = :user_id LIMIT 1",
            named_params! {
                ":peer_id": peer_id,
                ":user_id": user_id,
            },
            map_row,
        )
        .await {
        Ok(status) => Ok(status.unwrap_or(VerifyStatus::Null)),
        Err(e) => Err(e),
    }
}

pub async fn set_status(
    peer_id: PeerId,
    user_id: PeerId,
    status: VerifyStatus,
) -> libsql::Result<()> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let user_id = user_id.bot_api_dialog_id().unwrap();
    let (status, date, message_id, solution) = match status {
        VerifyStatus::Null => (None, None, None, None),
        VerifyStatus::Verifying {
            date,
            message_id,
            solution,
        } => (
            Some(1),
            Some(date.as_millisecond()),
            Some(message_id),
            Some(solution),
        ),
        VerifyStatus::Verified => (Some(2), None, None, None),
        VerifyStatus::Banned => (Some(3), None, None, None),
    };

    let db = Database::open().await?;
    let transaction = db.begin_transaction().await?;
    let stmt = transaction
        .prepare(
            "INSERT INTO verify (peer_id, user_id, status, date, message_id, solution)
                  VALUES (:peer_id, :user_id, :status, :date, :message_id, :solution)
                  ON CONFLICT(peer_id, user_id) DO UPDATE SET
                      status = excluded.status,
                      date = excluded.date,
                      message_id = excluded.message_id,
                      solution = excluded.solution;",
        )
        .await?;
    stmt.execute(named_params! {
        ":peer_id": peer_id,
        ":user_id": user_id,
        ":status": status,
        ":date": date,
        ":message_id": message_id,
        ":solution": solution,
    })
    .await?;
    transaction.commit().await?;

    Ok(())
}

static GET_VERIFYING_CONNECTION: OnceCell<Arc<Database>> = OnceCell::const_new();

async fn get_verifying_connection() -> libsql::Result<Arc<Database>> {
    GET_VERIFYING_CONNECTION
        .get_or_try_init(|| async { Ok(Arc::new(Database::open().await?)) })
        .await
        .map(|db| Arc::clone(&db))
}

pub async fn get_all_verifying() -> libsql::Result<Vec<(PeerId, PeerId, VerifyStatus)>> {
    // 复用同一条数据库连接
    let db = get_verifying_connection().await?;

    let map_row = |row: libsql::Row| {
        let peer_id: i64 = row.get(0)?;
        let peer_id = PeerId::from_bot_api_dialog_id(peer_id).unwrap();
        let user_id: i64 = row.get(1)?;
        let user_id = PeerId::from_bot_api_dialog_id(user_id).unwrap();
        let status_code: u32 = row.get(2)?;
        let date_ms: i64 = row.get(3)?;
        let message_id: i32 = row.get(4)?;
        let solution: u32 = row.get(5)?;

        // 虽然 WHERE 子句过滤了 status = 1，但为了类型安全还是进行匹配
        let status = match status_code {
            1 => VerifyStatus::Verifying {
                date: Timestamp::from_millisecond(date_ms).unwrap(),
                message_id,
                solution,
            },
            // 理论上不会进入其他分支，但为了枚举完整性保留处理
            2 => VerifyStatus::Verified,
            3 => VerifyStatus::Banned,
            _ => VerifyStatus::Null,
        };

        Ok((peer_id, user_id, status))
    };

    db.fetch_all(
        "SELECT peer_id, user_id, status, date, message_id, solution FROM verify WHERE status = 1",
        params![],
        map_row,
    )
    .await
}
