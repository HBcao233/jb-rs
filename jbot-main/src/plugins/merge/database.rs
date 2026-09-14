use std::sync::{Arc, OnceLock};

use grammers_session::types::PeerId;
use libsql::{Builder, Connection, Result, Value, named_params, params};
use tokio::fs;

static MERGE_DB: OnceLock<Arc<Database>> = OnceLock::new();

async fn merge_db() -> Result<Arc<Database>> {
    Ok(match MERGE_DB.get() {
        Some(db) => Arc::clone(&db),
        None => {
            let db = Arc::new(Database::open().await?);
            MERGE_DB.set(Arc::clone(&db)).unwrap();
            db
        }
    })
}

const VERSION: i64 = 1;

#[derive(Debug)]
struct Database(Connection);

impl Database {
    async fn open() -> libsql::Result<Self> {
        if let Err(e) = fs::create_dir_all("data").await {
            log::error!("数据文件夹创建失败: {e:?}");
            return Err(libsql::Error::ConnectionFailed(
                "数据文件夹创建失败".to_string(),
            ));
        }
        let conn = Builder::new_local("data/merge.db")
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
                "CREATE TABLE merge (
                id INTEGER PRIMARY KEY,
                peer_id INTEGER NOT NULL,
                message_id INTEGER NOT NULL)",
                params![],
            )
            .await?;
        transaction
            .execute("CREATE INDEX merge_peer_id ON merge (peer_id)", params![])
            .await?;

        transaction
            .execute(
                "CREATE TABLE pinned (
                id INTEGER PRIMARY KEY,
                peer_id INTEGER NOT NULL,
                message_id INTEGER NOT NULL)",
                params![],
            )
            .await?;
        transaction
            .execute("CREATE INDEX pinned_peer_id ON merge (peer_id)", params![])
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

pub async fn get_session(peer_id: PeerId) -> Result<Vec<i32>> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let db = merge_db().await?;
    let map_row = |row: libsql::Row| {
        let message_id = row.get::<i32>(2)?;
        Ok(message_id)
    };

    Ok(db
        .fetch_all(
            "SELECT * FROM merge WHERE peer_id = :peer_id",
            named_params! {":peer_id": peer_id},
            map_row,
        )
        .await?)
}

pub async fn insert_session(peer_id: PeerId, message_ids: Vec<i32>) -> Result<()> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let db = merge_db().await?;
    let transaction = db.begin_transaction().await?;

    let placeholders: Vec<String> = message_ids.iter().map(|_| "(?, ?)".to_string()).collect();
    let sql = format!(
        "INSERT INTO merge (peer_id, message_id) VALUES {}",
        placeholders.join(", ")
    );

    let mut params = Vec::with_capacity(message_ids.len() * 2);
    for message_id in &message_ids {
        params.push(Value::from(peer_id));
        params.push(Value::from(*message_id));
    }

    transaction.execute(&sql, params).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn finish_session(peer_id: PeerId) -> libsql::Result<()> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let db = merge_db().await?;
    let transaction = db.begin_transaction().await?;
    transaction
        .execute("DELETE FROM merge WHERE peer_id = ?", params![peer_id])
        .await?;
    transaction
        .execute("DELETE FROM pinned WHERE peer_id = ?", params![peer_id])
        .await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn get_pinned(peer_id: PeerId) -> Result<Vec<i32>> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let db = merge_db().await?;
    let map_row = |row: libsql::Row| {
        let message_id = row.get::<i32>(2)?;
        Ok(message_id)
    };

    Ok(db
        .fetch_all(
            "SELECT * FROM pinned WHERE peer_id = :peer_id",
            named_params! {":peer_id": peer_id},
            map_row,
        )
        .await?)
}

pub async fn insert_pinned(peer_id: PeerId, message_id: i32) -> Result<()> {
    let peer_id = peer_id.bot_api_dialog_id().unwrap();
    let db = merge_db().await?;
    let transaction = db.begin_transaction().await?;

    transaction
        .execute(
            "INSERT INTO pinned (peer_id, message_id) VALUES (?, ?)",
            params![peer_id, message_id],
        )
        .await?;
    transaction.commit().await?;
    Ok(())
}
