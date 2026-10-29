//! Drive C: kept in memory: files and folders of DOS programs where there is no host file system
//! (the browser). Paths are keys: upper-case names joined with `/`, no drive letter or leading
//! slash (`DATA/INPUT.TXT`); the root folder is the empty key.

use std::collections::{BTreeMap, BTreeSet};

use crate::dos::{E_ACCESS, E_FILE_NOT_FOUND, E_PATH_NOT_FOUND};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemDrive {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

/// Room on the drive for all its files: it is kept in browser storage (a few MB per site, as
/// base64, next to the saved programs).
pub const CAPACITY: usize = 2 * 1024 * 1024;

/// Most files and folders a DOS program can make (a full drive gives "access denied", as DOS).
const MAX_ENTRIES: usize = 1024;

/// Header of [`MemDrive::to_bytes`].
const MAGIC: &[u8; 8] = b"RE86DRV1";

/// Reads [`MemDrive::to_bytes`]: little-endian lengths, UTF-8 names.
struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(n)?;
        self.0 = rest;
        Some(head)
    }

    fn u32(&mut self) -> Option<usize> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?) as usize)
    }

    fn string(&mut self) -> Option<String> {
        let n = self.u32()?;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }
}

impl MemDrive {
    pub fn new() -> Self {
        Self::default()
    }

    /// Key of a path given by the user (`\` or `/`, any case, an optional drive letter); None when it
    /// is empty or contains `.` / `..`.
    pub fn normalize(path: &str) -> Option<String> {
        let mut p = path.trim().replace('\\', "/");
        if p.len() >= 2 && p.as_bytes()[1] == b':' {
            p = p[2..].to_string();
        }
        let parts: Vec<&str> = p.split('/').filter(|c| !c.is_empty()).collect();
        if parts.is_empty() || parts.iter().any(|c| *c == "." || *c == "..") {
            return None;
        }
        Some(parts.join("/").to_ascii_uppercase())
    }

    /// Files with their keys, in key order.
    pub fn files(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.files.iter().map(|(k, v)| (k.as_str(), v.as_slice()))
    }

    /// Folders (keys), in key order.
    pub fn dirs(&self) -> impl Iterator<Item = &str> {
        self.dirs.iter().map(String::as_str)
    }

    /// Bytes taken by the files.
    pub fn used(&self) -> usize {
        self.files.values().map(Vec::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.dirs.is_empty()
    }

    /// Contents of the file at `path`.
    pub fn read(&self, path: &str) -> Option<&[u8]> {
        self.files.get(&Self::normalize(path)?).map(Vec::as_slice)
    }

    /// Stores a file (replacing one with the same name), creating its folders; returns its key.
    pub fn write(&mut self, path: &str, data: Vec<u8>) -> Result<String, String> {
        let key = Self::normalize(path).ok_or_else(|| format!("'{path}' is not a file name"))?;
        if self.dirs.contains(&key) {
            return Err(format!("{key} is a folder"));
        }
        let others = self.used() - self.files.get(&key).map_or(0, Vec::len);
        if others + data.len() > CAPACITY {
            return Err(format!(
                "Not enough room for {key} ({} KB): drive C: holds {} KB, {} KB are free.",
                data.len().div_ceil(1024),
                CAPACITY / 1024,
                CAPACITY.saturating_sub(others) / 1024
            ));
        }
        let mut dir = String::new();
        for part in key.split('/').take(key.split('/').count() - 1) {
            if !dir.is_empty() {
                dir.push('/');
            }
            dir.push_str(part);
            if self.files.contains_key(&dir) {
                return Err(format!("{dir} is a file"));
            }
            self.dirs.insert(dir.clone());
        }
        self.files.insert(key.clone(), data);
        Ok(key)
    }

    /// Deletes the file or the folder (with everything in it) at `path`.
    pub fn remove(&mut self, path: &str) -> bool {
        let Some(key) = Self::normalize(path) else { return false };
        if self.files.remove(&key).is_some() {
            return true;
        }
        if !self.dirs.remove(&key) {
            return false;
        }
        let prefix = format!("{key}/");
        self.files.retain(|k, _| !k.starts_with(&prefix));
        self.dirs.retain(|k| !k.starts_with(&prefix));
        true
    }

    /// The drive as bytes, for keeping it in browser storage ([`MemDrive::from_bytes`] reads it).
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = MAGIC.to_vec();
        out.extend((self.dirs.len() as u32).to_le_bytes());
        for d in &self.dirs {
            out.extend((d.len() as u32).to_le_bytes());
            out.extend(d.as_bytes());
        }
        out.extend((self.files.len() as u32).to_le_bytes());
        for (k, v) in &self.files {
            out.extend((k.len() as u32).to_le_bytes());
            out.extend(k.as_bytes());
            out.extend((v.len() as u32).to_le_bytes());
            out.extend(v);
        }
        out
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut r = Reader(bytes.strip_prefix(MAGIC.as_slice())?);
        let mut drive = MemDrive::new();
        for _ in 0..r.u32()? {
            drive.dirs.insert(r.string()?);
        }
        for _ in 0..r.u32()? {
            let key = r.string()?;
            let n = r.u32()?;
            drive.files.insert(key, r.take(n)?.to_vec());
        }
        r.0.is_empty().then_some(drive)
    }

    // ---- DOS operations (keys; errors are DOS error codes) ---------------------------------

    fn full(&self) -> bool {
        self.files.len() + self.dirs.len() >= MAX_ENTRIES
    }

    fn parent(key: &str) -> &str {
        key.rsplit_once('/').map_or("", |(p, _)| p)
    }

    pub(crate) fn is_dir(&self, key: &str) -> bool {
        key.is_empty() || self.dirs.contains(key)
    }

    pub(crate) fn file(&self, key: &str) -> Option<&Vec<u8>> {
        self.files.get(key)
    }

    /// AH=40h: writes `data` at `at`, zero-filling a gap before it; empty `data` sets the length
    /// to `at`. Only what fits on the drive is written (DOS writes fewer bytes when the disk is
    /// full). Returns the number of bytes written; None when there is no such file.
    pub(crate) fn write_at(&mut self, key: &str, at: u64, data: &[u8]) -> Option<usize> {
        let used = self.used();
        let file = self.files.get_mut(key)?;
        // The longest this file may get.
        let room = CAPACITY.saturating_sub(used - file.len()).max(file.len()) as u64;
        if data.is_empty() {
            let len = if at <= file.len() as u64 { at } else { at.min(room) };
            file.resize(len as usize, 0);
            return Some(0);
        }
        let end = (at + data.len() as u64).min(room);
        if end <= at {
            return Some(0);
        }
        let (at, end) = (at as usize, end as usize);
        if file.len() < end {
            file.resize(end, 0);
        }
        file[at..end].copy_from_slice(&data[..end - at]);
        Some(end - at)
    }

    /// The error for a key that does not exist: its folder is missing too, or just the file.
    fn missing(&self, key: &str) -> u16 {
        if self.is_dir(Self::parent(key)) { E_FILE_NOT_FOUND } else { E_PATH_NOT_FOUND }
    }

    /// AH=3Dh: the file must exist.
    pub(crate) fn open(&self, key: &str) -> Result<(), u16> {
        if self.files.contains_key(key) {
            Ok(())
        } else if self.is_dir(key) {
            Err(E_ACCESS)
        } else {
            Err(self.missing(key))
        }
    }

    /// AH=3Ch (an existing file becomes empty) and AH=5Bh (`new_only`: it must not exist).
    pub(crate) fn create(&mut self, key: &str, new_only: bool) -> Result<(), u16> {
        if self.is_dir(key) || (new_only && self.files.contains_key(key)) {
            return Err(E_ACCESS);
        }
        if !self.is_dir(Self::parent(key)) {
            return Err(E_PATH_NOT_FOUND);
        }
        if !self.files.contains_key(key) && self.full() {
            return Err(E_ACCESS);
        }
        self.files.insert(key.to_string(), Vec::new());
        Ok(())
    }

    /// AH=39h.
    pub(crate) fn create_dir(&mut self, key: &str) -> Result<(), u16> {
        if self.is_dir(key) || self.files.contains_key(key) {
            return Err(E_ACCESS);
        }
        if !self.is_dir(Self::parent(key)) {
            return Err(E_PATH_NOT_FOUND);
        }
        if self.full() {
            return Err(E_ACCESS);
        }
        self.dirs.insert(key.to_string());
        Ok(())
    }

    /// AH=3Ah: the folder must be empty.
    pub(crate) fn remove_dir(&mut self, key: &str) -> Result<(), u16> {
        if !self.dirs.contains(key) {
            return Err(E_PATH_NOT_FOUND);
        }
        let prefix = format!("{key}/");
        if self.files.keys().chain(self.dirs.iter()).any(|k| k.starts_with(&prefix)) {
            return Err(E_ACCESS);
        }
        self.dirs.remove(key);
        Ok(())
    }

    /// AH=41h.
    pub(crate) fn remove_file(&mut self, key: &str) -> Result<(), u16> {
        match self.files.remove(key) {
            Some(_) => Ok(()),
            None if self.is_dir(key) => Err(E_ACCESS),
            None => Err(self.missing(key)),
        }
    }

    /// AH=56h: renames or moves a file or a folder; the new name must not exist.
    pub(crate) fn rename(&mut self, from: &str, to: &str) -> Result<(), u16> {
        if self.is_dir(to) || self.files.contains_key(to) {
            return Err(E_ACCESS);
        }
        if !self.is_dir(Self::parent(to)) {
            return Err(E_PATH_NOT_FOUND);
        }
        if let Some(data) = self.files.remove(from) {
            self.files.insert(to.to_string(), data);
            return Ok(());
        }
        if from.is_empty() || !self.dirs.contains(from) {
            return Err(self.missing(from));
        }
        let (old, new) = (format!("{from}/"), format!("{to}/"));
        if to.starts_with(&old) {
            return Err(E_ACCESS);
        }
        let moved = |k: &String| k.strip_prefix(&old).map(|rest| format!("{new}{rest}"));
        self.files = std::mem::take(&mut self.files).into_iter().map(|(k, v)| (moved(&k).unwrap_or(k), v)).collect();
        self.dirs = std::mem::take(&mut self.dirs).into_iter().map(|k| moved(&k).unwrap_or(k)).collect();
        self.dirs.remove(from);
        self.dirs.insert(to.to_string());
        Ok(())
    }

    /// Files directly in folder `dir`: name and size; None when it is not a folder.
    pub(crate) fn list(&self, dir: &str) -> Option<Vec<(String, u64)>> {
        if !self.is_dir(dir) {
            return None;
        }
        let prefix = if dir.is_empty() { String::new() } else { format!("{dir}/") };
        Some(
            self.files
                .iter()
                .filter_map(|(k, v)| {
                    let name = k.strip_prefix(&prefix)?;
                    (!name.contains('/')).then(|| (name.to_string(), v.len() as u64))
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_paths_and_folders() {
        let mut d = MemDrive::new();
        assert_eq!(MemDrive::normalize("c:\\data\\in.txt").as_deref(), Some("DATA/IN.TXT"));
        assert_eq!(MemDrive::normalize("a/../b"), None);
        assert_eq!(d.write("data/in.txt", b"42".to_vec()).as_deref(), Ok("DATA/IN.TXT"));
        assert!(d.is_dir("DATA"));
        assert_eq!(d.read("DATA\\IN.TXT"), Some(&b"42"[..]));
        assert!(d.write("data/in.txt/x", vec![]).is_err());
        assert_eq!(d.list("DATA"), Some(vec![("IN.TXT".to_string(), 2)]));
        assert_eq!(d.list(""), Some(vec![]));
        assert!(d.remove("data"));
        assert!(d.is_empty());
    }

    #[test]
    fn the_drive_has_a_capacity() {
        let mut d = MemDrive::new();
        d.create("A.TXT", false).unwrap();
        // A write far past the end: only what fits, the gap zero-filled.
        assert_eq!(d.write_at("A.TXT", 0x7FFF_FFFF, b"x"), Some(0));
        assert_eq!(d.write_at("A.TXT", CAPACITY as u64 - 2, b"xyz"), Some(2));
        assert_eq!(d.used(), CAPACITY);
        assert_eq!(d.read("A.TXT").unwrap()[CAPACITY - 3..], [0, b'x', b'y']);
        // Overwriting inside the file still works; growing another file does not.
        assert_eq!(d.write_at("A.TXT", 0, b"ab"), Some(2));
        d.create("B.TXT", false).unwrap();
        assert_eq!(d.write_at("B.TXT", 0, b"b"), Some(0));
        assert!(d.write("C.TXT", vec![1]).unwrap_err().contains("0 KB are free"));
        // A zero-length write truncates (and can't extend past the capacity).
        assert_eq!(d.write_at("A.TXT", 10, b""), Some(0));
        assert_eq!(d.used(), 10);
        assert_eq!(d.write_at("A.TXT", u32::MAX as u64, b""), Some(0));
        assert_eq!(d.used(), CAPACITY);
        assert_eq!(d.write_at("NONE", 0, b"x"), None);
        // Files and folders: up to MAX_ENTRIES.
        let mut d = MemDrive::new();
        for i in 0..MAX_ENTRIES {
            d.create(&format!("F{i}"), false).unwrap();
        }
        assert_eq!(d.create("ONE.MORE", false), Err(E_ACCESS));
        assert_eq!(d.create_dir("DIR"), Err(E_ACCESS));
        d.create("F0", false).unwrap();
    }

    #[test]
    fn dos_errors() {
        let mut d = MemDrive::new();
        assert_eq!(d.open("A.TXT"), Err(E_FILE_NOT_FOUND));
        assert_eq!(d.open("NO/A.TXT"), Err(E_PATH_NOT_FOUND));
        assert_eq!(d.create("NO/A.TXT", false), Err(E_PATH_NOT_FOUND));
        d.create("A.TXT", false).unwrap();
        assert_eq!(d.create("A.TXT", true), Err(E_ACCESS));
        d.create_dir("SUB").unwrap();
        assert_eq!(d.create_dir("SUB"), Err(E_ACCESS));
        d.rename("A.TXT", "SUB/B.TXT").unwrap();
        assert_eq!(d.remove_dir("SUB"), Err(E_ACCESS));
        d.rename("SUB", "NEW").unwrap();
        assert!(d.file("NEW/B.TXT").is_some());
        assert_eq!(d.rename("NEW", "NEW/INSIDE"), Err(E_ACCESS));
        d.remove_file("NEW/B.TXT").unwrap();
        d.remove_dir("NEW").unwrap();
        assert!(d.is_empty());
    }

    #[test]
    fn bytes_round_trip() {
        let mut d = MemDrive::new();
        d.write("x/y.bin", vec![0, 1, 2, 255]).unwrap();
        d.create_dir("EMPTY").unwrap();
        let bytes = d.to_bytes();
        assert_eq!(MemDrive::from_bytes(&bytes), Some(d));
        assert_eq!(MemDrive::from_bytes(&bytes[..bytes.len() - 1]), None);
        assert_eq!(MemDrive::from_bytes(b"junk"), None);
    }
}
