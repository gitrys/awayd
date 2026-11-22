//! SPDX-License-Identifier: GPL-3.0-or-later
//! Tail live log file + lazy prepend from rotated siblings on scroll-up.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::log::{parse_log_bytes, LogEntry};

pub const CHUNK_BYTES: u64 = 64 * 1024;
pub const MAX_LOADED_ENTRIES: usize = 20_000;

/// Ordered list of log files: index 0 = active `awayd.log`, then `.1`, `.2`, …
pub fn log_file_chain(logs_dir: &Path) -> Vec<PathBuf> {
    let mut chain = vec![logs_dir.join("awayd.log")];
    for i in 1..=32 {
        let p = logs_dir.join(format!("awayd.log.{i}"));
        if p.is_file() {
            chain.push(p);
        } else {
            break;
        }
    }
    chain
}

pub struct TailReader {
    logs_dir: PathBuf,
    file_chain: Vec<PathBuf>,
    /// Index into `file_chain` for the next older file to load on scroll-up.
    next_older_idx: usize,
    /// Byte offset in `file_chain[0]` for incremental tail.
    tail_offset: u64,
    /// Start of the first line of the active file as read (identifies the file: after a
    /// rotation `awayd.log` starts with another line, even when it is already longer than
    /// `tail_offset`). File creation times are no help: Windows file tunneling gives a file
    /// re-created under the same name the old creation time.
    head: Vec<u8>,
    pub entries: Vec<LogEntry>,
}

/// Bytes of the first line kept in [`TailReader::head`] (the timestamp makes it unique).
const HEAD_BYTES: usize = 256;

impl TailReader {
    pub fn new(logs_dir: PathBuf) -> Self {
        let file_chain = log_file_chain(&logs_dir);
        Self {
            logs_dir,
            file_chain,
            next_older_idx: 1,
            tail_offset: 0,
            head: Vec::new(),
            entries: Vec::new(),
        }
    }

    /// Start over with the (new) active file.
    fn reset_after_rotation(&mut self) {
        self.tail_offset = 0;
        self.head.clear();
        self.entries.clear();
        self.next_older_idx = 1;
    }

    /// The active file still starts with the line read before.
    fn same_file(&self, f: &mut File) -> bool {
        if self.head.is_empty() {
            return true;
        }
        let mut start = vec![0u8; self.head.len()];
        f.seek(SeekFrom::Start(0)).is_ok() && f.read_exact(&mut start).is_ok() && start == self.head
    }

    pub fn refresh_chain(&mut self) {
        self.file_chain = log_file_chain(&self.logs_dir);
    }

    pub fn has_older(&self) -> bool {
        self.next_older_idx < self.file_chain.len()
    }

    /// Read new bytes from the active log since last poll.
    pub fn poll_tail(&mut self) -> usize {
        self.refresh_chain();
        let Some(active) = self.file_chain.first() else {
            return 0;
        };
        if !active.is_file() {
            return 0;
        }
        let Ok(mut f) = File::open(active) else {
            return 0;
        };
        let len = f.metadata().map(|m| m.len()).unwrap_or(0);
        // Rotated: shorter than what was read, or another first line.
        if len < self.tail_offset || !self.same_file(&mut f) {
            self.reset_after_rotation();
        }
        if len <= self.tail_offset {
            return 0;
        }
        let from_start = self.tail_offset == 0;
        let _ = f.seek(SeekFrom::Start(self.tail_offset));
        let to_read = (len - self.tail_offset) as usize;
        let mut buf = vec![0u8; to_read.min(256 * 1024)];
        let n = f.read(&mut buf).unwrap_or(0);
        buf.truncate(n);
        self.tail_offset += n as u64;
        if n == 0 {
            return 0;
        }
        if self.tail_offset > 0 && n > 0 && buf.last() != Some(&b'\n') {
            if let Some(pos) = buf.iter().rposition(|&b| b == b'\n') {
                let remainder = buf.split_off(pos + 1);
                self.tail_offset -= remainder.len() as u64;
                buf.truncate(pos + 1);
            } else {
                self.tail_offset -= n as u64;
                return 0;
            }
        }
        if from_start {
            let first_line = buf.iter().position(|&b| b == b'\n').unwrap_or(buf.len());
            self.head = buf[..first_line.min(HEAD_BYTES)].to_vec();
        }
        let parsed = parse_log_bytes(&buf);
        let count = parsed.len();
        self.entries.extend(parsed);
        self.trim_entries();
        count
    }

    /// Load older lines from the next rotated file (prepend). Call when user scrolls to top.
    pub fn prepend_older_chunk(&mut self) -> usize {
        if !self.has_older() {
            return 0;
        }
        let path = self.file_chain[self.next_older_idx].clone();
        self.next_older_idx += 1;
        let chunk = read_tail_chunk(&path, CHUNK_BYTES);
        let parsed = parse_log_bytes(&chunk);
        let count = parsed.len();
        if count > 0 {
            let mut merged = parsed;
            merged.append(&mut self.entries);
            self.entries = merged;
            self.trim_entries();
        }
        count
    }

    fn trim_entries(&mut self) {
        if self.entries.len() > MAX_LOADED_ENTRIES {
            let drop = self.entries.len() - MAX_LOADED_ENTRIES;
            self.entries.drain(0..drop);
        }
    }
}

fn read_tail_chunk(path: &Path, max_bytes: u64) -> Vec<u8> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    if len == 0 {
        return Vec::new();
    }
    let start = len.saturating_sub(max_bytes);
    let _ = f.seek(SeekFrom::Start(start));
    let mut buf = Vec::new();
    let _ = f.read_to_end(&mut buf);
    if start > 0 {
        if let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            buf.drain(0..=pos);
        }
    }
    buf
}

#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn prepend_older_adds_lines() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("awayd.log");
        let old = dir.path().join("awayd.log.1");
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&old).unwrap();
            writeln!(
                f,
                r#"{{"v":1,"ts":"2026-06-12T08:00:00.000","level":"INFO","target":"t","message":"old"}}"#
            )
            .unwrap();
        }
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&active).unwrap();
            writeln!(
                f,
                r#"{{"v":1,"ts":"2026-06-12T08:05:00.000","level":"INFO","target":"t","message":"new"}}"#
            )
            .unwrap();
        }

        let mut tail = TailReader::new(dir.path().to_path_buf());
        tail.poll_tail();
        assert_eq!(tail.entries.len(), 1);
        assert_eq!(tail.entries[0].message, "new");

        let n = tail.prepend_older_chunk();
        assert_eq!(n, 1);
        assert_eq!(tail.entries.len(), 2);
        assert_eq!(tail.entries[0].message, "old");
        assert_eq!(tail.entries[1].message, "new");
    }

    fn line(msg: &str) -> String {
        format!(
            r#"{{"v":1,"ts":"2026-11-22T20:00:00.000+01:00","level":"INFO","target":"t","message":"{msg}"}}"#
        )
    }

    fn write_lines(path: &std::path::Path, msgs: &[&str]) {
        use std::io::Write;
        let mut f = std::fs::File::create(path).unwrap();
        for m in msgs {
            writeln!(f, "{}", line(m)).unwrap();
        }
    }

    /// The new `awayd.log` is already longer than the old read position at the next poll:
    /// detected by its first line, all its lines are read, nothing from the middle.
    #[test]
    fn rotation_is_detected_when_new_file_is_already_longer() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("awayd.log");
        write_lines(&active, &["old-1", "old-2"]);

        let mut tail = TailReader::new(dir.path().to_path_buf());
        assert_eq!(tail.poll_tail(), 2);

        std::fs::rename(&active, dir.path().join("awayd.log.1")).unwrap();
        let new: Vec<String> = (1..=6).map(|i| format!("new-{i}")).collect();
        let refs: Vec<&str> = new.iter().map(String::as_str).collect();
        write_lines(&active, &refs);

        tail.poll_tail();
        let msgs: Vec<&str> = tail.entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(msgs, refs);
        // "Load older" starts again with the file rotated just now.
        assert_eq!(tail.prepend_older_chunk(), 2);
        assert_eq!(tail.entries[0].message, "old-1");
    }

    #[test]
    fn appended_lines_are_not_mistaken_for_rotation() {
        let dir = tempdir().unwrap();
        let active = dir.path().join("awayd.log");
        write_lines(&active, &["a"]);
        let mut tail = TailReader::new(dir.path().to_path_buf());
        assert_eq!(tail.poll_tail(), 1);
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(&active)
                .unwrap();
            writeln!(f, "{}", line("b")).unwrap();
        }
        assert_eq!(tail.poll_tail(), 1);
        let msgs: Vec<&str> = tail.entries.iter().map(|e| e.message.as_str()).collect();
        assert_eq!(msgs, ["a", "b"]);
    }
}
