use std::collections::VecDeque;

use chrono::{DateTime, Utc};

use crate::ui::logs::LogLine;

/// Ring buffer for log history, enabling --tail and --since queries.
pub struct LogBuffer {
    lines: VecDeque<LogLine>,
    capacity: usize,
}

impl LogBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(capacity.min(1024)),
            capacity,
        }
    }

    /// Push a log line, evicting the oldest if at capacity.
    pub fn push(&mut self, line: LogLine) {
        if self.lines.len() >= self.capacity {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Return the last N lines.
    pub fn tail(&self, n: usize) -> Vec<&LogLine> {
        let skip = self.lines.len().saturating_sub(n);
        self.lines.iter().skip(skip).collect()
    }

    /// Return all lines with timestamp >= since.
    pub fn since(&self, since: DateTime<Utc>) -> Vec<&LogLine> {
        self.lines.iter().filter(|l| l.timestamp >= since).collect()
    }

    /// Return all lines.
    pub fn all(&self) -> Vec<&LogLine> {
        self.lines.iter().collect()
    }

    /// Current number of lines in the buffer.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_line(service: &str, ts: DateTime<Utc>) -> LogLine {
        LogLine {
            timestamp: ts,
            service: service.to_string(),
            text: "test".to_string(),
            is_stderr: false,
            level: None,
        }
    }

    #[test]
    fn buffer_capacity_eviction() {
        let mut buf = LogBuffer::new(3);
        let now = Utc::now();
        buf.push(make_line("a", now));
        buf.push(make_line("b", now));
        buf.push(make_line("c", now));
        assert_eq!(buf.len(), 3);

        buf.push(make_line("d", now));
        assert_eq!(buf.len(), 3);
        assert_eq!(buf.all()[0].service, "b"); // "a" was evicted
    }

    #[test]
    fn buffer_tail() {
        let mut buf = LogBuffer::new(10);
        let now = Utc::now();
        for i in 0..5 {
            buf.push(make_line(&format!("s{}", i), now));
        }
        let tail = buf.tail(3);
        assert_eq!(tail.len(), 3);
        assert_eq!(tail[0].service, "s2");
        assert_eq!(tail[1].service, "s3");
        assert_eq!(tail[2].service, "s4");
    }

    #[test]
    fn buffer_since() {
        let mut buf = LogBuffer::new(10);
        let t0 = Utc::now() - Duration::seconds(60);
        let t1 = Utc::now() - Duration::seconds(30);
        let t2 = Utc::now();

        buf.push(make_line("old", t0));
        buf.push(make_line("mid", t1));
        buf.push(make_line("new", t2));

        let cutoff = Utc::now() - Duration::seconds(45);
        let result = buf.since(cutoff);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].service, "mid");
        assert_eq!(result[1].service, "new");
    }

    #[test]
    fn buffer_empty() {
        let buf = LogBuffer::new(10);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert!(buf.all().is_empty());
        assert!(buf.tail(5).is_empty());
    }
}
