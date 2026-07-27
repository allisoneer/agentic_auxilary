pub const SANITIZE: &str = "SELECT 1";
pub const ASSERT_CLEAN_BEGIN: &str = "BEGIN DEFERRED";
pub const ASSERT_CLEAN_ROLLBACK: &str = "ROLLBACK";

pub const UPSERT_QUALIFICATION_PROBE: &str = "INSERT INTO __attention_probe \
    (operation_id, fingerprint, value) VALUES (?1, ?2, ?3) \
    ON CONFLICT(operation_id) DO NOTHING";

pub const SELECT_QUALIFICATION_PROBE: &str = "SELECT fingerprint, value \
    FROM __attention_probe WHERE operation_id = ?1";
