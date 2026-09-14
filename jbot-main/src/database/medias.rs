use std::sync::{Arc, OnceLock};
use tokio::fs;

use grammers_client::media::{Document, Media};
use grammers_client::message::Message;
use grammers_tl_types as tl;
use libsql::{Builder, Connection};
use libsql::{named_params, params};

static MEDIAS_DB: OnceLock<Arc<Database>> = OnceLock::new();

async fn medias_db() -> libsql::Result<Arc<Database>> {
    Ok(match MEDIAS_DB.get() {
        Some(db) => Arc::clone(&db),
        None => {
            let db = Arc::new(Database::open().await?);
            MEDIAS_DB.set(Arc::clone(&db)).unwrap();
            db
        }
    })
}

const VERSION: i64 = 1;

#[derive(Debug)]
struct Database(Connection);

#[repr(u8)]
enum MediaType {
    Photo = 1,
    Video = 2,
    Audio = 3,
    Document = 4,
    Animated = 5,
    Sticker = 6,
    CustomEmoji = 7,
}

impl TryFrom<u8> for MediaType {
    type Error = ();

    fn try_from(x: u8) -> Result<Self, ()> {
        Ok(match x {
            v if v == Self::Photo as u8 => Self::Photo,
            v if v == Self::Video as u8 => Self::Video,
            v if v == Self::Audio as u8 => Self::Audio,
            v if v == Self::Document as u8 => Self::Document,
            v if v == Self::Animated as u8 => Self::Animated,
            v if v == Self::Sticker as u8 => Self::Sticker,
            v if v == Self::CustomEmoji as u8 => Self::CustomEmoji,
            _ => return Err(()),
        })
    }
}

impl Database {
    async fn open() -> libsql::Result<Self> {
        if let Err(e) = fs::create_dir_all("data").await {
            log::error!("数据文件夹创建失败: {e:?}");
            return Err(libsql::Error::ConnectionFailed(
                "数据文件夹创建失败".to_string(),
            ));
        }
        let conn = Builder::new_local("data/medias.db")
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
                "CREATE TABLE medias (
                id INTEGER PRIMARY KEY,
                peer_id INTEGER,
                message_id INTEGER,
                grouped_id INTEGER,
                media_type INTEGER,
                file_id INTEGER NOT NULL UNIQUE,
                access_hash INTEGER NOT NULL,
                file_reference BLOB NOT NULL,
                key TEXT UNIQUE)",
                params![],
            )
            .await?;
        transaction
            .execute(
                "CREATE INDEX medias_peer_id ON medias (peer_id, message_id, grouped_id)",
                params![],
            )
            .await?;
        transaction
            .execute("CREATE INDEX medias_key ON medias (key)", params![])
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

    /*
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
    */
}

pub async fn insert_from_message(message: &Message, key: Option<&str>) -> libsql::Result<()> {
    let peer_id = message.peer_id().bot_api_dialog_id().unwrap();
    let message_id = message.id();
    let grouped_id = message.grouped_id();
    let (media_type, file_id, access_hash, file_reference) = if let Some(media) = message.media() {
        match media {
            Media::Photo(photo) => {
                if let tl::enums::InputPhoto::Photo(tl::types::InputPhoto {
                    id,
                    access_hash,
                    file_reference,
                }) = photo.to_raw_input_photo()
                {
                    (MediaType::Photo, id, access_hash, file_reference)
                } else {
                    return Ok(());
                }
            }
            Media::Document(document) => {
                if let Some(media_type) = parse_media_type(&document) {
                    if let tl::enums::InputDocument::Document(tl::types::InputDocument {
                        id,
                        access_hash,
                        file_reference,
                    }) = document.to_raw_input_document()
                    {
                        (media_type, id, access_hash, file_reference)
                    } else {
                        return Ok(());
                    }
                } else {
                    return Ok(());
                }
            }
            _ => {
                return Ok(());
            }
        }
    } else {
        return Ok(());
    };

    let db = medias_db().await?;
    let transaction = db.begin_transaction().await?;
    let stmt = transaction
        .prepare("INSERT INTO medias (peer_id, message_id, grouped_id, media_type, file_id, access_hash, file_reference, key)
                  VALUES (:peer_id, :message_id, :grouped_id, :media_type, :file_id, :access_hash, :file_reference, :key)
                  ON CONFLICT(file_id) DO UPDATE SET
                      peer_id = excluded.peer_id,
                      message_id = excluded.message_id,
                      grouped_id = excluded.grouped_id,
                      key = COALESCE(excluded.key, medias.key);")
        .await?;
    stmt.execute(named_params! {
        ":peer_id": peer_id,
        ":message_id": message_id,
        ":grouped_id": grouped_id,
        ":media_type": media_type as u8,
        ":file_id": file_id,
        ":access_hash": access_hash,
        ":file_reference": file_reference,
        ":key": key,
    })
    .await?;
    transaction.commit().await?;

    Ok(())
}

fn parse_media_type(document: &Document) -> Option<MediaType> {
    match document.raw.document.as_ref() {
        Some(tl::enums::Document::Document(d)) => {
            use tl::enums::DocumentAttribute as DA;
            for attr in &d.attributes {
                match attr {
                    DA::Video(_) => return Some(MediaType::Video),
                    DA::Audio(_) => return Some(MediaType::Audio),
                    DA::Animated => return Some(MediaType::Animated),
                    DA::Sticker(_) => return Some(MediaType::Sticker),
                    DA::CustomEmoji(_) => return Some(MediaType::CustomEmoji),
                    _ => {}
                }
            }
            Some(MediaType::Document)
        }
        _ => None,
    }
}

pub async fn get_media(key: &str) -> libsql::Result<Option<tl::enums::InputMedia>> {
    let db = medias_db().await?;
    let map_row = |row: libsql::Row| {
        let media_type: MediaType = (row.get::<u32>(4)? as u8).try_into().unwrap();
        let file_id = row.get::<i64>(5)?;
        let access_hash = row.get::<i64>(6)?;
        let file_reference = row.get::<Vec<u8>>(7)?;
        Ok(match media_type {
            MediaType::Photo => tl::types::InputMediaPhoto {
                spoiler: false,
                live_photo: false,
                id: tl::types::InputPhoto {
                    id: file_id,
                    access_hash,
                    file_reference,
                }
                .into(),
                ttl_seconds: None,
                video: None,
            }
            .into(),
            _ => tl::types::InputMediaDocument {
                spoiler: false,
                id: tl::types::InputDocument {
                    id: file_id,
                    access_hash,
                    file_reference,
                }
                .into(),
                video_cover: None,
                video_timestamp: None,
                ttl_seconds: None,
                query: None,
            }
            .into(),
        })
    };

    Ok(db
        .fetch_one(
            "SELECT * FROM medias WHERE key = :key LIMIT 1",
            named_params! {":key": key},
            map_row,
        )
        .await?)
}
