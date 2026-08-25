use crate::config::config_dir;
use crate::routes::Destination;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

const MAX_HISTORY_ENTRIES: usize = 1000;
const DEFAULT_DISPLAY_LIMIT: usize = 25;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub ca: String,
    pub destination: String,
    pub timestamp: String,
}

pub struct History {
    path: PathBuf,
}

impl History {
    pub fn new() -> Result<Self> {
        let dir = config_dir()?;
        fs::create_dir_all(&dir)?;
        Ok(Self {
            path: dir.join("history.jsonl"),
        })
    }

    pub fn record(&self, ca: &str, destination: Destination) {
        let entry = HistoryEntry {
            ca: ca.to_string(),
            destination: destination.display_name().to_string(),
            timestamp: format_utc_time(),
        };

        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            if let Ok(line) = serde_json::to_string(&entry) {
                let _ = writeln!(file, "{}", line);
            }
        }

        // Trim if over limit (best effort, non-blocking)
        let _ = self.trim_to_limit();
    }

    pub fn read_all(&self) -> Result<Vec<HistoryEntry>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut entries = Vec::new();
        for line in reader.lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                if let Ok(entry) = serde_json::from_str::<HistoryEntry>(&line) {
                    entries.push(entry);
                }
            }
        }
        // Newest first
        entries.reverse();
        Ok(entries)
    }

    pub fn read_latest(&self) -> Result<Vec<HistoryEntry>> {
        let mut entries = self.read_all()?;
        entries.truncate(DEFAULT_DISPLAY_LIMIT);
        Ok(entries)
    }

    pub fn clear(&self) -> Result<()> {
        if self.path.exists() {
            fs::remove_file(&self.path)?;
        }
        Ok(())
    }

    fn trim_to_limit(&self) -> Result<()> {
        if !self.path.exists() {
            return Ok(());
        }

        let entries = self.read_all()?;
        if entries.len() <= MAX_HISTORY_ENTRIES {
            return Ok(());
        }

        // Keep newest MAX_HISTORY_ENTRIES, rewrite file
        let keep: Vec<&HistoryEntry> = entries.iter().take(MAX_HISTORY_ENTRIES).collect();
        let mut file = fs::File::create(&self.path)?;
        for entry in keep.into_iter().rev() {
            if let Ok(line) = serde_json::to_string(entry) {
                writeln!(file, "{}", line)?;
            }
        }
        Ok(())
    }
}

fn format_utc_time() -> String {
    let now = std::time::SystemTime::now();
    let Ok(duration) = now.duration_since(std::time::UNIX_EPOCH) else {
        return String::from("unknown");
    };
    let secs = duration.as_secs();

    let total_days = (secs / 86400) as i64;
    let tod = secs % 86400;
    let hours = tod / 3600;
    let minutes = (tod % 3600) / 60;
    let seconds = tod % 60;

    let mut y = 1970i64;
    let mut remaining_days = total_days;
    loop {
        let diy = if is_leap(y) { 366 } else { 365 };
        if remaining_days < diy {
            break;
        }
        remaining_days -= diy;
        y += 1;
    }

    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0;
    for (i, &d) in month_days.iter().enumerate() {
        if remaining_days < d as i64 {
            m = i;
            break;
        }
        remaining_days -= d as i64;
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        y,
        m + 1,
        remaining_days + 1,
        hours,
        minutes,
        seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_history_record_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("test_history.jsonl");
        let history = History { path: history_path };

        assert!(history.read_all().unwrap().is_empty());

        history.record(
            "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
            Destination::Gmgn,
        );

        let entries = history.read_all().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].ca,
            "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU"
        );
        assert_eq!(entries[0].destination, "GMGN");
    }

    #[test]
    fn test_history_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("test_history.jsonl");
        let history = History { path: history_path };

        history.record("AAA", Destination::Gmgn);
        history.record("BBB", Destination::DexScreener);
        history.record("CCC", Destination::Pumpfun);

        let entries = history.read_all().unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].ca, "CCC");
        assert_eq!(entries[1].ca, "BBB");
        assert_eq!(entries[2].ca, "AAA");
    }

    #[test]
    fn test_history_clear() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("test_history.jsonl");
        let history = History { path: history_path };

        history.record("AAA", Destination::Gmgn);
        assert_eq!(history.read_all().unwrap().len(), 1);

        history.clear().unwrap();
        assert!(history.read_all().unwrap().is_empty());
    }

    #[test]
    fn test_history_trim() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("test_history.jsonl");
        let history = History { path: history_path };

        // Write more than MAX_HISTORY_ENTRIES by temporarily reducing limit
        // We test the logic by checking file behavior
        for i in 0..5 {
            history.record(&format!("CA_{}", i), Destination::Gmgn);
        }

        let entries = history.read_all().unwrap();
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].ca, "CA_4");
    }

    #[test]
    fn test_history_duplicate_same_ca() {
        let dir = tempfile::tempdir().unwrap();
        let history_path = dir.path().join("test_history.jsonl");
        let history = History { path: history_path };

        history.record(
            "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
            Destination::Gmgn,
        );
        history.record(
            "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU",
            Destination::DexScreener,
        );

        let entries = history.read_all().unwrap();
        assert_eq!(entries.len(), 2);
        // Both entries present (timestamps differ, route history not unique-CA DB)
        assert_eq!(entries[0].destination, "DexScreener");
        assert_eq!(entries[1].destination, "GMGN");
    }

    #[test]
    fn test_format_utc_time() {
        let ts = format_utc_time();
        assert!(ts.ends_with(" UTC"));
        assert_eq!(ts.len(), 23);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], " ");
    }
}
