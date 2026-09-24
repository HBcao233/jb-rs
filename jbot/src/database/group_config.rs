use grammers_session::types::PeerId;
use libsql::{Builder, Connection};
use libsql::{named_params, params};
use tokio::fs;
use tracing::error;

const VERSION: i64 = 1;

#[derive(Debug)]
struct Database(Connection);

impl Database {
    async fn open() -> libsql::Result<Self> {
        if let Err(e) = fs::create_dir_all("data").await {
            error!("数据文件夹创建失败: {e:?}");
            return Err(libsql::Error::ConnectionFailed(
                "数据文件夹创建失败".to_string(),
            ));
        }
        let conn = Builder::new_local("data/group_config.db")
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
                "CREATE TABLE group_config (
                id INTEGER PRIMARY KEY,
                peer_id INTEGER,
                key TEXT,
                value TEXT,
                UNIQUE(peer_id, key))",
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
}

pub async fn get_config(peer_id: PeerId, key: &str) -> libsql::Result<Option<String>> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();

    let db = Database::open().await?;
    let map_row = |row: libsql::Row| row.get::<String>(0);

    Ok(db
        .fetch_one(
            "SELECT value FROM group_config WHERE peer_id = :peer_id AND key = :key LIMIT 1",
            named_params! {
                ":peer_id": peer_id,
                ":key": key,
            },
            map_row,
        )
        .await?)
}

pub async fn set_config(peer_id: PeerId, key: &str, value: &str) -> libsql::Result<()> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();

    let db = Database::open().await?;
    let transaction = db.begin_transaction().await?;
    let stmt = transaction
        .prepare(
            "INSERT INTO group_config (peer_id, key, value)
                  VALUES (:peer_id, :key, :value)
                  ON CONFLICT(peer_id, key) DO UPDATE SET
                      value = excluded.value;",
        )
        .await?;
    stmt.execute(named_params! {
        ":peer_id": peer_id,
        ":key": key,
        ":value": value,
    })
    .await?;
    transaction.commit().await?;

    Ok(())
}
