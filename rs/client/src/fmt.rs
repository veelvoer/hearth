//! Formatting helpers for time, duration, and pace.

const WEEK_MS: i64 = 7 * 86_400_000;

/// Format milliseconds as a time-ago string: "now", "5m", "3h", "2d", or a date.
pub fn age(ms: i64) -> String {
    match ms {
        0..=90_000 => "now".to_string(),
        90_001..=3_600_000 => format!("{}m", (ms + 30_000) / 60_000),
        3_600_001..=86_400_000 => format!("{}h", (ms + 1_800_000) / 3_600_000),
        86_400_001..=WEEK_MS => format!("{}d", (ms + 43_200_000) / 86_400_000),
        _ => {
            let now = chrono::Local::now();
            let past = now - chrono::Duration::milliseconds(ms);
            past.format("%d %b").to_string()
        }
    }
}

/// Format milliseconds as a duration: "5m", "2h 30m", "1d 2h".
pub fn dur(ms: i64) -> String {
    let m = (ms / 60_000).max(0);
    match m {
        0..=59 => format!("{}m", m),
        60..=1439 => format!("{}h {}m", m / 60, m % 60),
        _ => format!("{}d {}h", m / 1440, (m % 1440) / 60),
    }
}

/// Format epoch milliseconds as HH:MM.
pub fn clock(ms: i64) -> String {
    let secs = ms / 1000;
    let mins = (secs / 60) % 60;
    let hours = secs / 3600;
    format!("{:02}:{:02}", hours, mins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_age() {
        assert_eq!(age(0), "now");
        assert_eq!(age(30_000), "now");
        assert_eq!(age(100_000), "2m");
        assert_eq!(age(1_000_000), "17m");
    }

    #[test]
    fn test_dur() {
        assert_eq!(dur(60_000), "1m");
        assert_eq!(dur(3_600_000), "1h 0m");
        assert_eq!(dur(5_400_000), "1h 30m");
        assert_eq!(dur(86_400_000), "1d 0h");
        assert_eq!(dur(90_000_000), "1d 1h");
    }

    #[test]
    fn test_clock() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(3661000), "01:01");
        assert_eq!(clock(36000000), "10:00");
    }
}
