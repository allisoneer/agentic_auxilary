use chrono::DateTime;
use chrono::Timelike;
use chrono::Utc;

pub fn truncate_to_microseconds(value: DateTime<Utc>) -> DateTime<Utc> {
    value
        .with_nanosecond((value.timestamp_subsec_nanos() / 1_000) * 1_000)
        .unwrap_or(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .expect("timestamp")
            .with_timezone(&Utc)
    }

    #[test]
    fn exact_microseconds_are_unchanged() {
        let value = at("2026-08-14T12:34:56.123456Z");
        assert_eq!(truncate_to_microseconds(value), value);
    }

    #[test]
    fn sub_microsecond_nanoseconds_are_truncated() {
        assert_eq!(
            truncate_to_microseconds(at("2026-08-14T12:34:56.123456789Z")),
            at("2026-08-14T12:34:56.123456Z")
        );
    }
}
