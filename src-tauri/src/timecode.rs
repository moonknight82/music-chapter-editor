use regex::Regex;

pub fn parse_timestamp_ns(value: &str) -> Result<u64, String> {
    let regex = Regex::new(r"^(\d+):([0-5]\d):([0-5]\d)(?:\.(\d{1,9}))?$").unwrap();
    let captures = regex
        .captures(value.trim())
        .ok_or_else(|| "Use HH:MM:SS with up to nine fractional digits".to_string())?;
    let hours = captures[1]
        .parse::<u64>()
        .map_err(|_| "Hours are too large".to_string())?;
    let minutes = captures[2].parse::<u64>().unwrap();
    let seconds = captures[3].parse::<u64>().unwrap();
    let fraction = captures.get(4).map(|m| m.as_str()).unwrap_or("");
    let mut nanos_text = fraction.to_string();
    nanos_text.push_str(&"0".repeat(9 - nanos_text.len()));
    let nanos = if nanos_text.is_empty() {
        0
    } else {
        nanos_text.parse::<u64>().unwrap()
    };
    hours
        .checked_mul(3_600_000_000_000)
        .and_then(|v| v.checked_add(minutes * 60_000_000_000))
        .and_then(|v| v.checked_add(seconds * 1_000_000_000))
        .and_then(|v| v.checked_add(nanos))
        .ok_or_else(|| "Timestamp is too large".to_string())
}

pub fn format_timestamp_ns(value: u64) -> String {
    let total_seconds = value / 1_000_000_000;
    let nanos = value % 1_000_000_000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if nanos == 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}.000")
    } else {
        let fraction = format!("{nanos:09}").trim_end_matches('0').to_string();
        format!("{hours:02}:{minutes:02}:{seconds:02}.{fraction}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_nanoseconds() {
        assert_eq!(
            parse_timestamp_ns("01:02:03.123456789").unwrap(),
            3_723_123_456_789
        );
        assert_eq!(format_timestamp_ns(3_723_123_456_789), "01:02:03.123456789");
        assert_eq!(
            format_timestamp_ns(parse_timestamp_ns("00:00:01.5").unwrap()),
            "00:00:01.5"
        );
    }

    #[test]
    fn rejects_invalid_time_components() {
        assert!(parse_timestamp_ns("00:60:00").is_err());
        assert!(parse_timestamp_ns("00:00:60").is_err());
        assert!(parse_timestamp_ns("1:02").is_err());
    }
}
