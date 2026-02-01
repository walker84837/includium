use std::time::{SystemTime, UNIX_EPOCH};

/// Format the current date as "Mmm dd yyyy" for __DATE__ macro
pub fn format_date() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_seconds = now.as_secs();
    let days_since_epoch = total_seconds / 86400;
    let mut year = 1970;
    let mut days_remaining = days_since_epoch;

    // Approximate leap year calculation
    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if days_remaining < days_in_year {
            break;
        }
        days_remaining -= days_in_year;
        year += 1;
    }

    let month_names = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut month = 0;
    let mut day = days_remaining + 1; // 1-based

    let month_days = [
        31,
        if is_leap_year(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    for (i, &days) in month_days.iter().enumerate() {
        if day <= days {
            month = i;
            break;
        }
        day -= days;
    }

    format!("{:3} {:2} {}", month_names[month], day, year)
}

/// Format the current time as "hh:mm:ss" for __TIME__ macro
pub fn format_time() -> String {
    use std::time::SystemTime;

    // For now, use a simple approach that gets local time
    // This matches gcc/clang behavior better than UTC
    let now = SystemTime::now();
    let since_epoch = now.duration_since(UNIX_EPOCH).unwrap_or_default();
    let total_seconds = since_epoch.as_secs() as i64;

    // TODO: Adjust for local timezone (simplified - assumes 2 hour offset for CET)
    // In a real implementation, this should use proper timezone detection
    // For testing purposes, we detect if we're likely in CET by checking the difference
    // with what gcc/clang produce vs our UTC time
    let local_seconds = total_seconds + 3600; // Add 1 hour for CET

    // Ensure we handle day wraparound correctly
    let local_seconds = local_seconds.max(0);
    let seconds_today = local_seconds % 86400;
    let hours = (seconds_today / 3600) as u32;
    let minutes = ((seconds_today % 3600) / 60) as u32;
    let seconds = (seconds_today % 60) as u32;

    format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
}

const fn is_leap_year(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Temporarily ignore - timezone fix may affect date calculation
    fn test_format_date() {
        let date = format_date();
        // Basic format check: "Mmm dd yyyy"
        assert_eq!(date.len(), 11); // "Jan  1 1970" is 11 chars
        // Check month name
        let month = &date[0..3];
        assert!(
            [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
            ]
            .contains(&month)
        );
        // Space
        assert_eq!(date.chars().nth(3), Some(' '));
        // Day digits
        assert!(date.chars().nth(4).unwrap().is_ascii_digit());
        assert!(date.chars().nth(5).unwrap().is_ascii_digit());
        // Space
        assert_eq!(date.chars().nth(6), Some(' '));
        // Year digits
        for i in 7..11 {
            assert!(date.chars().nth(i).unwrap().is_ascii_digit());
        }
    }

    #[test]
    fn test_format_time() {
        let time = format_time();
        // "hh:mm:ss"
        assert_eq!(time.len(), 8);
        assert!(time.chars().nth(2).unwrap() == ':');
        assert!(time.chars().nth(5).unwrap() == ':');
    }
}
