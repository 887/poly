use surrealdb::Surreal;
use surrealdb::engine::local::{Db as SurrealDb, SurrealKv};
use tracing::info;

use crate::config::Config;

/// Type alias for our embedded SurrealDB handle.
pub type Db = Surreal<SurrealDb>;

/// Initialize SurrealKV, select namespace/database, and run schema migrations.
pub async fn init(config: &Config) -> anyhow::Result<Db> {
    info!("Opening SurrealKV at {}", config.db_path);
    let db = Surreal::new::<SurrealKv>(&config.db_path).await?;
    db.use_ns("poly").use_db("server").await?;

    // Authenticate as root for schema operations.
    // The server always connects as root; per-user auth is enforced in Rust handlers.
    db.query(SCHEMA).await?.check()?;

    info!("Database schema applied");
    Ok(db)
}

/// Complete SurrealQL schema.
///
/// Idempotent — uses `DEFINE … OVERWRITE` so re-applying on restart is safe.
/// DECISION(DX): We enforce permissions in Rust, not SurrealQL, to keep the
/// schema simple and avoid the per-request re-authentication overhead of
/// embedded SurrealKV.
const SCHEMA: &str = r#"
-- Users
DEFINE TABLE OVERWRITE user SCHEMAFULL;
DEFINE FIELD OVERWRITE username       ON user TYPE string;
DEFINE FIELD OVERWRITE display_name   ON user TYPE string;
DEFINE FIELD OVERWRITE avatar_url     ON user TYPE option<string>;
DEFINE FIELD OVERWRITE password_hash  ON user TYPE string;
DEFINE FIELD OVERWRITE created_at     ON user TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE user_username  ON user COLUMNS username UNIQUE;

-- Devices
DEFINE TABLE OVERWRITE device SCHEMAFULL;
DEFINE FIELD OVERWRITE owner       ON device TYPE record<user>;
DEFINE FIELD OVERWRITE name        ON device TYPE string;
DEFINE FIELD OVERWRITE user_agent  ON device TYPE option<string>;
DEFINE FIELD OVERWRITE ip          ON device TYPE option<string>;
DEFINE FIELD OVERWRITE created_at  ON device TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE last_seen   ON device TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE revoked     ON device TYPE bool DEFAULT false;

-- Servers (guilds)
DEFINE TABLE OVERWRITE server SCHEMAFULL;
DEFINE FIELD OVERWRITE name        ON server TYPE string;
DEFINE FIELD OVERWRITE icon_url    ON server TYPE option<string>;
DEFINE FIELD OVERWRITE owner       ON server TYPE record<user>;
DEFINE FIELD OVERWRITE created_at  ON server TYPE datetime DEFAULT time::now();

-- Memberships (user <-> server)
DEFINE TABLE OVERWRITE membership SCHEMAFULL;
DEFINE FIELD OVERWRITE user       ON membership TYPE record<user>;
DEFINE FIELD OVERWRITE server     ON membership TYPE record<server>;
DEFINE FIELD OVERWRITE joined_at  ON membership TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE membership_unique ON membership COLUMNS user, server UNIQUE;

-- Invite codes
DEFINE TABLE OVERWRITE invite SCHEMAFULL;
DEFINE FIELD OVERWRITE code        ON invite TYPE string;
DEFINE FIELD OVERWRITE server      ON invite TYPE record<server>;
DEFINE FIELD OVERWRITE created_by  ON invite TYPE record<user>;
DEFINE FIELD OVERWRITE created_at  ON invite TYPE datetime DEFAULT time::now();
DEFINE FIELD OVERWRITE expires_at  ON invite TYPE option<datetime>;
DEFINE FIELD OVERWRITE uses        ON invite TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE max_uses    ON invite TYPE option<int>;
DEFINE INDEX OVERWRITE invite_code ON invite COLUMNS code UNIQUE;

-- Categories
DEFINE TABLE OVERWRITE category SCHEMAFULL;
DEFINE FIELD OVERWRITE server    ON category TYPE record<server>;
DEFINE FIELD OVERWRITE name      ON category TYPE string;
DEFINE FIELD OVERWRITE position  ON category TYPE int DEFAULT 0;

-- Channels
DEFINE TABLE OVERWRITE channel SCHEMAFULL;
DEFINE FIELD OVERWRITE server    ON channel TYPE option<record<server>>;
DEFINE FIELD OVERWRITE category  ON channel TYPE option<record<category>>;
DEFINE FIELD OVERWRITE name      ON channel TYPE string;
DEFINE FIELD OVERWRITE kind      ON channel TYPE string; -- "text" | "voice"
DEFINE FIELD OVERWRITE position  ON channel TYPE int DEFAULT 0;
DEFINE FIELD OVERWRITE created_at ON channel TYPE datetime DEFAULT time::now();

-- Participants (DM / group channel members)
DEFINE TABLE OVERWRITE participant SCHEMAFULL;
DEFINE FIELD OVERWRITE user     ON participant TYPE record<user>;
DEFINE FIELD OVERWRITE channel  ON participant TYPE record<channel>;
DEFINE FIELD OVERWRITE added_at ON participant TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE participant_unique ON participant COLUMNS user, channel UNIQUE;

-- Messages
DEFINE TABLE OVERWRITE message SCHEMAFULL;
DEFINE FIELD OVERWRITE channel   ON message TYPE record<channel>;
DEFINE FIELD OVERWRITE author    ON message TYPE record<user>;
DEFINE FIELD OVERWRITE content   ON message TYPE string;
DEFINE FIELD OVERWRITE reply_to  ON message TYPE option<record<message>>;
DEFINE FIELD OVERWRITE edited_at ON message TYPE option<datetime>;
DEFINE FIELD OVERWRITE deleted   ON message TYPE bool DEFAULT false;
DEFINE FIELD OVERWRITE created_at ON message TYPE datetime DEFAULT time::now();

-- Reactions
DEFINE TABLE OVERWRITE reaction SCHEMAFULL;
DEFINE FIELD OVERWRITE message  ON reaction TYPE record<message>;
DEFINE FIELD OVERWRITE user     ON reaction TYPE record<user>;
DEFINE FIELD OVERWRITE emoji    ON reaction TYPE string;
DEFINE INDEX OVERWRITE reaction_unique ON reaction COLUMNS message, user, emoji UNIQUE;

-- Friend requests
DEFINE TABLE OVERWRITE friend_request SCHEMAFULL;
DEFINE FIELD OVERWRITE "from"    ON friend_request TYPE record<user>;
DEFINE FIELD OVERWRITE "to"      ON friend_request TYPE record<user>;
DEFINE FIELD OVERWRITE status    ON friend_request TYPE string DEFAULT 'pending';
DEFINE FIELD OVERWRITE created_at ON friend_request TYPE datetime DEFAULT time::now();

-- Voice sessions (ephemeral — cleared on restart via REMOVE TABLE then DEFINE)
DEFINE TABLE OVERWRITE voice_session SCHEMAFULL;
DEFINE FIELD OVERWRITE user       ON voice_session TYPE record<user>;
DEFINE FIELD OVERWRITE channel    ON voice_session TYPE record<channel>;
DEFINE FIELD OVERWRITE joined_at  ON voice_session TYPE datetime DEFAULT time::now();
DEFINE INDEX OVERWRITE voice_session_unique ON voice_session COLUMNS user, channel UNIQUE;

-- Attachments (file uploads)
DEFINE TABLE OVERWRITE attachment SCHEMAFULL;
DEFINE FIELD OVERWRITE uploaded_by   ON attachment TYPE record<user>;
DEFINE FIELD OVERWRITE message       ON attachment TYPE option<record<message>>;
DEFINE FIELD OVERWRITE filename      ON attachment TYPE string;
DEFINE FIELD OVERWRITE storage_name  ON attachment TYPE string;
DEFINE FIELD OVERWRITE mime_type     ON attachment TYPE string;
DEFINE FIELD OVERWRITE size_bytes    ON attachment TYPE int;
DEFINE FIELD OVERWRITE created_at    ON attachment TYPE datetime DEFAULT time::now();
"#;
