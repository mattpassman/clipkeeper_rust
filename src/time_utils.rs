use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current UTC time as milliseconds since the Unix epoch.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before Unix epoch")
        .as_millis() as i64
}

/// Formats a Unix-epoch millisecond timestamp as an RFC 3339 string (e.g. `2025-03-11T14:30:00Z`).
pub fn millis_to_rfc3339(ms: i64) -> String {
    let total_secs = ms / 1000;
    let (year, month, day, hour, min, sec) = secs_to_utc(total_secs);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hour, min, sec)
}

/// Formats a Unix-epoch millisecond timestamp as `YYYY-MM-DD HH:MM:SS`.
pub fn millis_to_datetime(ms: i64) -> String {
    let total_secs = ms / 1000;
    let (year, month, day, hour, min, sec) = secs_to_utc(total_secs);
    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, min, sec)
}

/// Parses a subset of RFC 3339 timestamps (e.g. `2025-03-11T14:30:00Z` or
/// `2025-03-11T14:30:00+00:00`) and returns milliseconds since the Unix epoch.
/// Returns `None` on parse failure.
pub fn parse_rfc3339_to_millis(s: &str) -> Option<i64> {
    // Minimal parser: YYYY-MM-DDTHH:MM:SS followed by Z or +HH:MM / -HH:MM
    if s.len() < 20 {
        return None;
    }
    let year: i64 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let min: u32 = s[14..16].parse().ok()?;
    let sec: u32 = s[17..19].parse().ok()?;

    let epoch_secs = date_to_epoch_secs(year, month, day, hour, min, sec);

    // Parse timezone offset
    let tz_part = &s[19..];
    let offset_secs: i64 = if tz_part.starts_with('Z') {
        0
    } else if tz_part.len() >= 6 && (tz_part.starts_with('+') || tz_part.starts_with('-')) {
        let sign: i64 = if tz_part.starts_with('+') { 1 } else { -1 };
        let oh: i64 = tz_part[1..3].parse().ok()?;
        let om: i64 = tz_part[4..6].parse().ok()?;
        sign * (oh * 3600 + om * 60)
    } else {
        return None;
    };

    Some((epoch_secs - offset_secs) * 1000)
}

/// Returns the millisecond timestamp for the start of "today" in UTC.
pub fn today_start_millis() -> i64 {
    let total_secs = now_millis() / 1000;
    let (year, month, day, _, _, _) = secs_to_utc(total_secs);
    date_to_epoch_secs(year as i64, month, day, 0, 0, 0) * 1000
}

/// Returns the millisecond timestamp for the start of "yesterday" in UTC.
pub fn yesterday_start_millis() -> i64 {
    today_start_millis() - 86_400_000
}

// ── internal helpers ──

fn secs_to_utc(mut secs: i64) -> (i32, u32, u32, u32, u32, u32) {
    let sec = (secs.rem_euclid(60)) as u32;
    secs = secs.div_euclid(60);
    let min = (secs.rem_euclid(60)) as u32;
    secs = secs.div_euclid(60);
    let hour = (secs.rem_euclid(24)) as u32;
    let mut days = secs.div_euclid(24);

    // Civil date from day count (algorithm from Howard Hinnant)
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u32; // day of era [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m, d, hour, min, sec)
}

fn date_to_epoch_secs(year: i64, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> i64 {
    // Inverse of secs_to_utc using the same Hinnant algorithm
    let y = if month <= 2 { year - 1 } else { year };
    let m = if month <= 2 { month + 9 } else { month - 3 };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * m + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe as i64 - 719_468;
    days * 86400 + hour as i64 * 3600 + min as i64 * 60 + sec as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_millis_is_positive() {
        assert!(now_millis() > 0);
    }

    #[test]
    fn test_rfc3339_roundtrip() {
        let ms = 1_710_000_000_000i64; // 2024-03-09T16:00:00Z
        let s = millis_to_rfc3339(ms);
        assert_eq!(s, "2024-03-09T16:00:00Z");
        assert_eq!(parse_rfc3339_to_millis(&s), Some(ms));
    }

    #[test]
    fn test_millis_to_datetime() {
        let ms = 1_710_000_000_000i64;
        assert_eq!(millis_to_datetime(ms), "2024-03-09 16:00:00");
    }

    #[test]
    fn test_parse_rfc3339_with_offset() {
        // 2024-03-09T11:00:00-05:00 == 2024-03-09T16:00:00Z
        let ms = parse_rfc3339_to_millis("2024-03-09T11:00:00-05:00");
        assert_eq!(ms, Some(1_710_000_000_000));
    }

    #[test]
    fn test_today_yesterday() {
        let today = today_start_millis();
        let yesterday = yesterday_start_millis();
        assert_eq!(today - yesterday, 86_400_000);
        assert!(today <= now_millis());
    }

    #[test]
    fn test_epoch_zero() {
        assert_eq!(millis_to_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(millis_to_datetime(0), "1970-01-01 00:00:00");
    }
}
