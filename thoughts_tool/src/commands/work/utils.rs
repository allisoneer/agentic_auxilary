use chrono::{Datelike, NaiveDate, Utc};

/// Get the current ISO week directory name in format "YYYY-WWW" (e.g., "2025-W01")
pub fn current_iso_week_dir() -> String {
    let now = Utc::now().date_naive();
    let iso = now.iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

/// Get ISO week directory for a specific date in format "YYYY-WWW"
#[allow(dead_code)]
pub fn iso_week_dir_for_date(date: NaiveDate) -> String {
    let iso = date.iso_week();
    format!("{}-W{:02}", iso.year(), iso.week())
}

/// Legacy format (read-side compatibility): "YYYY_week_WW"
/// Used only for migration of existing directories created with old format.
#[allow(dead_code)]
pub fn legacy_iso_week_dir() -> String {
    let now = Utc::now().date_naive();
    let iso = now.iso_week();
    format!("{}_week_{:02}", iso.year(), iso.week())
}

/// Legacy format for a specific date (read-side compatibility): "YYYY_week_WW"
#[allow(dead_code)]
pub fn legacy_iso_week_dir_for_date(date: NaiveDate) -> String {
    let iso = date.iso_week();
    format!("{}_week_{:02}", iso.year(), iso.week())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn test_iso_week_format() {
        // Just verify the format is correct
        let week_dir = current_iso_week_dir();

        // Should match pattern: YYYY-WWW (e.g., "2025-W01")
        assert!(week_dir.contains("-W"), "Week dir should contain '-W'");
        let parts: Vec<&str> = week_dir.split("-W").collect();
        assert_eq!(parts.len(), 2);

        // Year should be 4 digits
        assert_eq!(parts[0].len(), 4);
        assert!(parts[0].parse::<u32>().is_ok());

        // Week should be 2 digits
        assert_eq!(parts[1].len(), 2);
        let week: u32 = parts[1].parse().unwrap();
        assert!((1..=53).contains(&week));
    }

    #[test]
    fn test_iso_week_formatting() {
        // Test a regular week
        let date = NaiveDate::from_ymd_opt(2025, 3, 15).unwrap();
        assert_eq!(iso_week_dir_for_date(date), "2025-W11");

        // Test week 1
        let date = NaiveDate::from_ymd_opt(2025, 1, 6).unwrap();
        assert_eq!(iso_week_dir_for_date(date), "2025-W02");

        // Test year boundary - Dec 31 that belongs to week 1 of next year
        let date = NaiveDate::from_ymd_opt(2024, 12, 30).unwrap();
        assert_eq!(iso_week_dir_for_date(date), "2025-W01");

        // Test year boundary - Jan 1 that belongs to last year's week
        let date = NaiveDate::from_ymd_opt(2021, 1, 1).unwrap();
        assert_eq!(iso_week_dir_for_date(date), "2020-W53");
    }

    #[test]
    fn test_legacy_iso_week_format() {
        // Verify the legacy format is still "YYYY_week_WW"
        let week_dir = legacy_iso_week_dir();

        let parts: Vec<&str> = week_dir.split('_').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[1], "week");

        // Year should be 4 digits
        assert_eq!(parts[0].len(), 4);
        assert!(parts[0].parse::<u32>().is_ok());

        // Week should be 2 digits
        assert_eq!(parts[2].len(), 2);
        let week: u32 = parts[2].parse().unwrap();
        assert!((1..=53).contains(&week));
    }

    #[test]
    fn test_legacy_iso_week_dir_for_date() {
        // Test legacy format for specific dates
        let date = NaiveDate::from_ymd_opt(2025, 3, 15).unwrap();
        assert_eq!(legacy_iso_week_dir_for_date(date), "2025_week_11");

        let date = NaiveDate::from_ymd_opt(2024, 12, 30).unwrap();
        assert_eq!(legacy_iso_week_dir_for_date(date), "2025_week_01");
    }

    #[test]
    fn test_iso_week_boundary_cases() {
        // Test some known boundary dates
        // Dec 31, 2024 is in ISO week 1 of 2025
        let date = NaiveDate::from_ymd_opt(2024, 12, 31).unwrap();
        let iso = date.iso_week();
        assert_eq!(iso.year(), 2025);
        assert_eq!(iso.week(), 1);

        // Jan 1, 2024 is in ISO week 1 of 2024
        let date = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        let iso = date.iso_week();
        assert_eq!(iso.year(), 2024);
        assert_eq!(iso.week(), 1);

        // Jan 1, 2023 is in ISO week 52 of 2022
        let date = NaiveDate::from_ymd_opt(2023, 1, 1).unwrap();
        let iso = date.iso_week();
        assert_eq!(iso.year(), 2022);
        assert_eq!(iso.week(), 52);
    }
}
