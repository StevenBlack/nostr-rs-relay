use crate::repo::postgres::PostgresPool;
use async_trait::async_trait;
use sqlx::{Executor, Postgres, Transaction};

#[async_trait]
pub trait Migration {
    fn serial_number(&self) -> i64;
    async fn run(&self, tx: &mut Transaction<Postgres>);
}

struct SimpleSqlMigration {
    pub serial_number: i64,
    pub sql: Vec<&'static str>,
}

#[async_trait]
impl Migration for SimpleSqlMigration {
    fn serial_number(&self) -> i64 {
        self.serial_number
    }

    async fn run(&self, tx: &mut Transaction<Postgres>) {
        for sql in self.sql.iter() {
            tx.execute(*sql).await.unwrap();
        }
    }
}

/// Execute all migrations on the database.
pub async fn run_migrations(db: &PostgresPool) -> crate::error::Result<usize> {
    prepare_migrations_table(db).await;
    run_migration(m001::migration(), db).await;
    Ok(m001::migration().serial_number() as usize)
}

async fn prepare_migrations_table(db: &PostgresPool) {
    sqlx::query("CREATE TABLE IF NOT EXISTS migrations (serial_number bigint)")
        .execute(db)
        .await
        .unwrap();
}

async fn run_migration(migration: impl Migration, db: &PostgresPool) {
    let row: i64 =
        sqlx::query_scalar("SELECT COUNT(*) AS count FROM migrations WHERE serial_number = $1")
            .bind(migration.serial_number())
            .fetch_one(db)
            .await
            .unwrap();

    if row > 0 {
        return;
    }

    let mut transaction = db.begin().await.unwrap();
    migration.run(&mut transaction).await;

    sqlx::query("INSERT INTO migrations VALUES ($1)")
        .bind(migration.serial_number())
        .execute(&mut transaction)
        .await
        .unwrap();

    transaction.commit().await.unwrap();
}

mod m001 {
    use crate::repo::postgres_migration::{Migration, SimpleSqlMigration};

    pub fn migration() -> impl Migration {
        SimpleSqlMigration {
            serial_number: 1,
            sql: vec![
                r#"
-- Events table
CREATE TABLE "event" (
	id bytea NOT NULL,
	pub_key bytea NOT NULL,
	created_at timestamp with time zone NOT NULL,
	kind integer NOT NULL,
	"content" bytea NOT NULL,
	hidden bit(1) NOT NULL DEFAULT 0::bit(1),
	delegated_by bytea NULL,
	first_seen timestamp with time zone NOT NULL DEFAULT now(),
	CONSTRAINT event_pkey PRIMARY KEY (id)
);
CREATE INDEX event_created_at_idx ON "event" (created_at,kind);
CREATE INDEX event_pub_key_idx ON "event" (pub_key);
CREATE INDEX event_delegated_by_idx ON "event" (delegated_by);

-- Tags table
CREATE TABLE "tag" (
	id int8 NOT NULL GENERATED BY DEFAULT AS IDENTITY,
	event_id bytea NOT NULL,
	"name" varchar NOT NULL,
	value bytea NOT NULL,
	CONSTRAINT tag_fk FOREIGN KEY (event_id) REFERENCES "event"(id) ON DELETE CASCADE
);
CREATE INDEX tag_event_id_idx ON tag USING btree (event_id, name);
CREATE UNIQUE INDEX tag_event_id_value_idx ON tag (event_id,name,value);
CREATE INDEX tag_value_idx ON tag USING btree (value);

-- NIP-05 Verfication table
CREATE TABLE "user_verification" (
	id int8 NOT NULL GENERATED BY DEFAULT AS IDENTITY,
	event_id bytea NOT NULL,
	"name" varchar NOT NULL,
	verified_at timestamptz NULL,
	failed_at timestamptz NULL,
	fail_count int4 NULL DEFAULT 0,
	CONSTRAINT user_verification_pk PRIMARY KEY (id),
	CONSTRAINT user_verification_fk FOREIGN KEY (event_id) REFERENCES "event"(id) ON DELETE CASCADE
);
CREATE INDEX user_verification_event_id_idx ON user_verification USING btree (event_id);
CREATE INDEX user_verification_name_idx ON user_verification USING btree (name);
        "#,
            ],
        }
    }
}
