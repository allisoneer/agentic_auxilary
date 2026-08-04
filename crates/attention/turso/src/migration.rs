use crate::Error;
use crate::decode;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashSet;
use turso_db::Connection;
use turso_db::params;
use turso_db::transaction::TransactionBehavior;

pub const MIGRATION_HEAD: u64 = 2;

#[derive(Debug, Clone, Copy)]
pub struct Migration {
    version: u64,
    name: &'static str,
    sql: &'static [u8],
}

impl Migration {
    pub const fn new(version: u64, name: &'static str, sql: &'static [u8]) -> Self {
        Self { version, name, sql }
    }

    pub const fn version(self) -> u64 {
        self.version
    }

    pub const fn name(self) -> &'static str {
        self.name
    }

    pub fn checksum(self) -> [u8; 32] {
        Sha256::digest(self.sql).into()
    }
}

pub const MIGRATIONS: &[Migration] = &[
    Migration::new(
        1,
        "foundation",
        include_bytes!("../migrations/0001_foundation.sql"),
    ),
    Migration::new(
        2,
        "attention_core",
        include_bytes!("../migrations/0002_attention_core.sql"),
    ),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MigrationReport {
    applied: usize,
    head: u64,
}

impl MigrationReport {
    pub const fn applied(self) -> usize {
        self.applied
    }

    pub const fn head(self) -> u64 {
        self.head
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "variants are deterministic test failpoints")
)]
pub enum Failpoint {
    AfterBody,
    AfterLedger,
    BeforeCommit,
    AfterCommit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use turso_db::Builder;

    #[test]
    fn manifest_rejects_duplicate_nonmonotonic_versions_and_names() {
        const SQL: &[u8] = b"SELECT 1";
        assert!(
            validate_manifest(&[Migration::new(1, "one", SQL), Migration::new(1, "two", SQL),])
                .is_err()
        );
        assert!(
            validate_manifest(&[Migration::new(2, "two", SQL), Migration::new(1, "one", SQL),])
                .is_err()
        );
        assert!(
            validate_manifest(&[
                Migration::new(1, "same", SQL),
                Migration::new(2, "same", SQL),
            ])
            .is_err()
        );
    }

    #[test]
    fn ledger_rejects_duplicate_unknown_too_new_name_and_checksum_drift() {
        let checksum = MIGRATIONS[0].checksum().to_vec();
        let valid = AppliedMigration {
            version: 1,
            name: "foundation".to_string(),
            checksum: checksum.clone(),
        };
        assert!(validate_ledger(std::slice::from_ref(&valid), MIGRATIONS).is_ok());
        assert!(validate_ledger(&[valid.clone(), valid], MIGRATIONS).is_err());
        assert!(
            validate_ledger(
                &[AppliedMigration {
                    version: 2,
                    name: "unknown".to_string(),
                    checksum: checksum.clone(),
                }],
                MIGRATIONS,
            )
            .is_err()
        );
        assert!(
            validate_ledger(
                &[AppliedMigration {
                    version: 1,
                    name: "renamed".to_string(),
                    checksum,
                }],
                MIGRATIONS,
            )
            .is_err()
        );
        assert!(
            validate_ledger(
                &[AppliedMigration {
                    version: 1,
                    name: "foundation".to_string(),
                    checksum: vec![0; 32],
                }],
                MIGRATIONS,
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn injected_failures_preserve_complete_ledger_and_schema_states() -> Result<(), Error> {
        for failpoint in [
            Failpoint::AfterBody,
            Failpoint::AfterLedger,
            Failpoint::BeforeCommit,
            Failpoint::AfterCommit,
        ] {
            let root = tempfile::tempdir().map_err(Error::Io)?;
            let path = root.path().join("migration.db");
            let path = path
                .to_str()
                .ok_or(Error::MigrationIntegrity("test path is not UTF-8"))?;
            let database = Builder::new_local(path).build().await?;
            let mut connection = database.connect()?;
            assert!(
                run_with_failpoint(&mut connection, failpoint)
                    .await
                    .is_err()
            );
            let applied = preflight(&connection).await?;
            let expected = usize::from(failpoint == Failpoint::AfterCommit);
            assert_eq!(applied.len(), expected);
            let report = run(&mut connection).await?;
            assert_eq!(report.applied(), MIGRATIONS.len() - expected);
            assert_eq!(preflight(&connection).await?.len(), MIGRATIONS.len());
        }
        Ok(())
    }

    #[tokio::test]
    async fn injected_failures_preserve_head_one_to_head_two_atomicity() -> Result<(), Error> {
        for failpoint in [
            Failpoint::AfterBody,
            Failpoint::AfterLedger,
            Failpoint::BeforeCommit,
            Failpoint::AfterCommit,
        ] {
            let root = tempfile::tempdir().map_err(Error::Io)?;
            let path = root.path().join("migration.db");
            let path = path
                .to_str()
                .ok_or(Error::MigrationIntegrity("test path is not UTF-8"))?;
            let database = Builder::new_local(path).build().await?;
            let mut connection = database.connect()?;
            connection
                .execute_batch(
                    std::str::from_utf8(MIGRATIONS[0].sql)
                        .map_err(|_| Error::MigrationIntegrity("migration SQL is not UTF-8"))?,
                )
                .await?;
            connection
                .execute(
                    "INSERT INTO __attention_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
                    params![
                        1_i64,
                        MIGRATIONS[0].name,
                        MIGRATIONS[0].checksum().to_vec()
                    ],
                )
                .await?;

            assert!(
                run_with_failpoint(&mut connection, failpoint)
                    .await
                    .is_err()
            );
            let applied = preflight(&connection).await?;
            let expected = 1 + usize::from(failpoint == Failpoint::AfterCommit);
            assert_eq!(applied.len(), expected);
            let report = run(&mut connection).await?;
            assert_eq!(report.applied(), MIGRATIONS.len() - expected);
            assert_eq!(preflight(&connection).await?.len(), MIGRATIONS.len());
        }
        Ok(())
    }

    #[test]
    fn migration_sql_remains_foundation_only() {
        let sql = std::str::from_utf8(MIGRATIONS[0].sql).expect("migration SQL is UTF-8");
        for excluded in [
            "WorkItem",
            "AttentionSignal",
            "Reminder",
            "SourceReceipt",
            "ChangeEvent",
            "Inbox",
            "Outbox",
            "DeliveryState",
        ] {
            assert!(!sql.contains(excluded));
        }
        let core = std::str::from_utf8(MIGRATIONS[1].sql).expect("migration SQL is UTF-8");
        assert!(core.contains("CREATE TABLE attention_stream_state"));
        assert!(core.contains("CREATE TABLE mutation_outcomes"));
        assert!(core.contains("CREATE TABLE change_events"));
        assert!(core.contains("CREATE TABLE outbox_intents"));
        for excluded in [
            "delivery_state",
            "lease",
            "checkpoint",
            "create table inbox",
        ] {
            assert!(!core.to_ascii_lowercase().contains(excluded));
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigration {
    version: u64,
    name: String,
    checksum: Vec<u8>,
}

pub fn validate_manifest(manifest: &[Migration]) -> Result<(), Error> {
    let mut prior = 0;
    let mut names = HashSet::new();
    for migration in manifest {
        if migration.version == 0 || migration.version <= prior {
            return Err(Error::MigrationIntegrity(
                "manifest versions must be strictly increasing and nonzero",
            ));
        }
        if !names.insert(migration.name) {
            return Err(Error::MigrationIntegrity("manifest names must be unique"));
        }
        if migration.sql.is_empty() {
            return Err(Error::MigrationIntegrity("migration SQL must not be empty"));
        }
        prior = migration.version;
    }
    Ok(())
}

pub async fn preflight(connection: &Connection) -> Result<Vec<AppliedMigration>, Error> {
    validate_manifest(MIGRATIONS)?;
    if !ledger_exists(connection).await? {
        return Ok(Vec::new());
    }
    let applied = read_ledger(connection).await?;
    validate_ledger(&applied, MIGRATIONS)?;
    Ok(applied)
}

async fn ledger_exists(connection: &Connection) -> Result<bool, Error> {
    let mut rows = connection
        .query(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = ?1",
            turso_db::params!["__attention_migrations"],
        )
        .await?;
    let row = rows
        .next()
        .await?
        .ok_or(Error::MigrationIntegrity("schema query returned no row"))?;
    Ok(decode::integer(&row, 0)? == 1)
}

pub async fn read_ledger(connection: &Connection) -> Result<Vec<AppliedMigration>, Error> {
    let mut rows = connection
        .query(
            "SELECT version, name, checksum FROM __attention_migrations ORDER BY version",
            (),
        )
        .await?;
    let mut applied = Vec::new();
    while let Some(row) = rows.next().await? {
        let version = decode::integer(&row, 0)?;
        let version = u64::try_from(version)
            .map_err(|_| Error::MigrationIntegrity("ledger version is negative"))?;
        applied.push(AppliedMigration {
            version,
            name: decode::text(&row, 1)?,
            checksum: decode::blob(&row, 2)?,
        });
    }
    Ok(applied)
}

pub fn validate_ledger(applied: &[AppliedMigration], manifest: &[Migration]) -> Result<(), Error> {
    let mut versions = HashSet::new();
    for (index, row) in applied.iter().enumerate() {
        if !versions.insert(row.version) {
            return Err(Error::MigrationIntegrity(
                "ledger contains duplicate versions",
            ));
        }
        let Some(expected) = manifest.get(index) else {
            return Err(Error::MigrationIntegrity(
                "database is newer than this binary",
            ));
        };
        if row.version != expected.version {
            if row.version > manifest.last().map_or(0, |migration| migration.version) {
                return Err(Error::MigrationIntegrity(
                    "database is newer than this binary",
                ));
            }
            return Err(Error::MigrationIntegrity(
                "ledger contains an unknown version",
            ));
        }
        if row.name != expected.name {
            return Err(Error::MigrationIntegrity("migration name drift detected"));
        }
        if row.checksum.as_slice() != expected.checksum() {
            return Err(Error::MigrationIntegrity(
                "migration checksum drift detected",
            ));
        }
    }
    Ok(())
}

pub async fn run(connection: &mut Connection) -> Result<MigrationReport, Error> {
    run_inner(connection, None).await
}

#[cfg(test)]
pub async fn run_with_failpoint(
    connection: &mut Connection,
    failpoint: Failpoint,
) -> Result<MigrationReport, Error> {
    run_inner(connection, Some(failpoint)).await
}

async fn run_inner(
    connection: &mut Connection,
    #[cfg_attr(not(test), expect(unused_variables))] failpoint: Option<Failpoint>,
) -> Result<MigrationReport, Error> {
    let applied = preflight(connection).await?;
    let mut applied_count = 0;
    for migration in &MIGRATIONS[applied.len()..] {
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .await?;
        let sql = std::str::from_utf8(migration.sql)
            .map_err(|_| Error::MigrationIntegrity("migration SQL is not UTF-8"))?;
        transaction.execute_batch(sql).await?;
        #[cfg(test)]
        if failpoint == Some(Failpoint::AfterBody) {
            return Err(Error::MigrationIntegrity("injected failure after body"));
        }
        let version = i64::try_from(migration.version)
            .map_err(|_| Error::MigrationIntegrity("migration version exceeds SQLite integer"))?;
        transaction
            .execute(
                "INSERT INTO __attention_migrations (version, name, checksum) VALUES (?1, ?2, ?3)",
                params![version, migration.name, migration.checksum().to_vec()],
            )
            .await?;
        #[cfg(test)]
        if failpoint == Some(Failpoint::AfterLedger) {
            return Err(Error::MigrationIntegrity("injected failure after ledger"));
        }
        #[cfg(test)]
        if failpoint == Some(Failpoint::BeforeCommit) {
            return Err(Error::MigrationIntegrity("injected failure before commit"));
        }
        transaction
            .commit()
            .await
            .map_err(|_| Error::CommitOutcomeUnknown)?;
        applied_count += 1;
        #[cfg(test)]
        if failpoint == Some(Failpoint::AfterCommit) {
            return Err(Error::CommitOutcomeUnknown);
        }
    }
    Ok(MigrationReport {
        applied: applied_count,
        head: MIGRATION_HEAD,
    })
}
