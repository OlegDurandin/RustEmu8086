//! Files of MIPS programs (syscalls 13..16), as MARS's `SystemIO`: descriptors 0..2 are the
//! console, open files get 3..31, flags 0 (read), 1 (write, create or truncate) and 9 (append).
//!
//! The files live in a sandbox: a folder on the computer or a store in memory (the browser).
//! A file name is relative to it; absolute names and `..` are refused, and so are symbolic links
//! leading out of the folder. Writing is limited by [`WRITE_QUOTA`], so that a program writing
//! forever does not fill the disk.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Descriptors MARS has, the console's 0..2 included.
pub const MAX_FILES: usize = 32;
/// Bytes a program may write to files in one run.
pub const WRITE_QUOTA: u64 = 16 * 1024 * 1024;

/// Opening flag: read only.
pub const O_RDONLY: u32 = 0;
/// Opening flag: write only (the file is created or truncated).
pub const O_WRONLY: u32 = 1;
/// Opening flag added to [`O_WRONLY`]: write at the end.
pub const O_APPEND: u32 = 8;

/// Where the files of programs are.
#[derive(Debug, Clone)]
pub enum Storage {
    /// No files: opening fails.
    None,
    /// A folder on the computer.
    Host(PathBuf),
    /// Files in memory, by name.
    Memory(BTreeMap<String, Vec<u8>>),
}

/// An open file.
#[derive(Debug)]
enum Handle {
    /// A file of the folder.
    Host(std::fs::File),
    /// A file in memory, read from `pos`.
    MemRead { name: String, pos: usize },
    /// A file in memory, written at its end.
    MemWrite { name: String },
}

/// The descriptor table and the storage.
#[derive(Debug)]
pub struct Files {
    /// Where files are.
    pub storage: Storage,
    /// Open files by descriptor (0..2 are the console and stay `None`): name, flags, handle.
    open: Vec<Option<(String, u32, Handle)>>,
    /// Bytes written in this run (see [`WRITE_QUOTA`]).
    written: u64,
    /// Grows when a file in memory is created or written: the interface copies the files to the
    /// browser's drive C: when it changes.
    pub generation: u64,
}

impl Files {
    /// No open files.
    pub fn new(storage: Storage) -> Files {
        Files { storage, open: (0..MAX_FILES).map(|_| None).collect(), written: 0, generation: 0 }
    }

    /// Closes every file (a new run).
    pub fn reset(&mut self) {
        self.open.iter_mut().for_each(|f| *f = None);
        self.written = 0;
    }

    /// The file a program names, inside the folder; None when the name leaves it.
    fn host_path(root: &Path, name: &str) -> Option<PathBuf> {
        let rel = Path::new(name);
        if name.is_empty() || rel.components().any(|c| !matches!(c, Component::Normal(_) | Component::CurDir)) {
            return None;
        }
        let path = root.join(rel);
        // The folder the file is in (it exists for an existing file or one being created) must
        // be inside the root, symbolic links resolved.
        let root = std::fs::canonicalize(root).ok()?;
        let dir = std::fs::canonicalize(path.parent()?).ok()?;
        if !dir.starts_with(&root) {
            return None;
        }
        let file = dir.join(path.file_name()?);
        match std::fs::symlink_metadata(&file) {
            Ok(m) if m.file_type().is_symlink() => {
                std::fs::canonicalize(&file).ok().filter(|p| p.starts_with(&root)).map(|_| file)
            }
            _ => Some(file),
        }
    }

    /// Opens `name` (syscall 13); the descriptor, or MARS's error message.
    pub fn open(&mut self, name: &str, flags: u32) -> Result<u32, String> {
        if self.open.iter().flatten().any(|(n, _, _)| n == name) {
            return Err(format!("File name {name} is already open."));
        }
        if !matches!(flags, O_RDONLY | O_WRONLY) && flags != O_WRONLY | O_APPEND {
            return Err(format!("File name {name} has unknown requested opening flag"));
        }
        let Some(fd) = (3..MAX_FILES).find(|&i| self.open[i].is_none()) else {
            return Err(format!("File name {name} exceeds maximum open file limit of {MAX_FILES}"));
        };
        let not_found = |dir: &str| format!("File {name} not found, open for {dir}.");
        let reading = flags == O_RDONLY;
        let handle = match &mut self.storage {
            Storage::None => return Err(not_found(if reading { "input" } else { "output" })),
            Storage::Host(root) => {
                let path = Self::host_path(root, name);
                let file = if reading {
                    path.and_then(|p| std::fs::File::open(p).ok()).ok_or_else(|| not_found("input"))?
                } else {
                    let mut o = std::fs::OpenOptions::new();
                    o.write(true).create(true);
                    if flags & O_APPEND != 0 {
                        o.append(true)
                    } else {
                        o.truncate(true)
                    };
                    path.and_then(|p| o.open(p).ok()).ok_or_else(|| not_found("output"))?
                };
                Handle::Host(file)
            }
            Storage::Memory(files) => {
                let key = Self::memory_name(name).ok_or_else(|| not_found(if reading { "input" } else { "output" }))?;
                // Names in any case, as on drive C: (whose files are shared with DOS programs).
                let key = files.keys().find(|k| k.eq_ignore_ascii_case(&key)).cloned().unwrap_or(key);
                if reading {
                    if !files.contains_key(&key) {
                        return Err(not_found("input"));
                    }
                    Handle::MemRead { name: key, pos: 0 }
                } else {
                    let f = files.entry(key.clone()).or_default();
                    if flags & O_APPEND == 0 {
                        f.clear();
                    }
                    self.generation += 1;
                    Handle::MemWrite { name: key }
                }
            }
        };
        self.open[fd] = Some((name.to_string(), flags, handle));
        Ok(fd as u32)
    }

    /// The key of a file in memory: `dir/file`, without `.` parts; None for names leaving the store.
    fn memory_name(name: &str) -> Option<String> {
        let mut parts = Vec::new();
        for c in Path::new(name).components() {
            match c {
                Component::Normal(p) => parts.push(p.to_str()?.to_string()),
                Component::CurDir => {}
                _ => return None,
            }
        }
        (!parts.is_empty()).then(|| parts.join("/"))
    }

    /// Reads up to `len` bytes from an open file (syscall 14 for descriptors 3..); 0 at its end.
    pub fn read(&mut self, fd: u32, len: usize) -> Result<Vec<u8>, String> {
        let not_open = || format!("File descriptor {fd} is not open for reading");
        let Some(Some((_, O_RDONLY, handle))) = self.open.get_mut(fd as usize) else { return Err(not_open()) };
        match handle {
            Handle::Host(f) => {
                let mut buf = vec![0; len];
                let n = f.read(&mut buf).map_err(|_| format!("IO Exception on read of file with fd {fd}"))?;
                buf.truncate(n);
                Ok(buf)
            }
            Handle::MemRead { name, pos } => {
                let Storage::Memory(files) = &self.storage else { return Err(not_open()) };
                let data = files.get(name).map_or(&[][..], |d| &d[..]);
                let start = (*pos).min(data.len());
                let out = data[start..(start + len).min(data.len())].to_vec();
                *pos = start + out.len();
                Ok(out)
            }
            Handle::MemWrite { .. } => Err(not_open()),
        }
    }

    /// Writes `data` to an open file (syscall 15 for descriptors 3..); the number of bytes.
    pub fn write(&mut self, fd: u32, data: &[u8]) -> Result<usize, String> {
        let not_open = || format!("File descriptor {fd} is not open for writing");
        let Some(Some((_, flags, handle))) = self.open.get_mut(fd as usize) else { return Err(not_open()) };
        if *flags & O_WRONLY == 0 {
            return Err(not_open());
        }
        if self.written + data.len() as u64 > WRITE_QUOTA {
            return Err(format!(
                "IO Exception on write of file with fd {fd}: the program wrote more than {} MB",
                WRITE_QUOTA >> 20
            ));
        }
        match handle {
            Handle::Host(f) => {
                f.write_all(data).map_err(|_| format!("IO Exception on write of file with fd {fd}"))?;
            }
            Handle::MemWrite { name } => {
                let Storage::Memory(files) = &mut self.storage else { return Err(not_open()) };
                files.entry(name.clone()).or_default().extend_from_slice(data);
                self.generation += 1;
            }
            Handle::MemRead { .. } => return Err(not_open()),
        }
        self.written += data.len() as u64;
        Ok(data.len())
    }

    /// Closes a descriptor (syscall 16); the console and unknown descriptors are left alone.
    pub fn close(&mut self, fd: u32) {
        if (3..MAX_FILES as u32).contains(&fd) {
            if let Some((_, _, Handle::Host(f))) = &mut self.open[fd as usize] {
                f.flush().ok();
            }
            self.open[fd as usize] = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_files() {
        let mut f = Files::new(Storage::Memory(BTreeMap::new()));
        assert_eq!(f.open("data.txt", O_RDONLY), Err("File data.txt not found, open for input.".into()));
        let w = f.open("data.txt", O_WRONLY).unwrap();
        assert_eq!(w, 3);
        assert_eq!(f.open("data.txt", O_RDONLY), Err("File name data.txt is already open.".into()));
        assert_eq!(f.write(w, b"hello"), Ok(5));
        assert!(f.read(w, 1).is_err());
        f.close(w);
        let a = f.open("./data.txt", O_WRONLY | O_APPEND).unwrap();
        f.write(a, b"!").unwrap();
        f.close(a);
        let r = f.open("data.txt", O_RDONLY).unwrap();
        assert_eq!(f.read(r, 3).unwrap(), b"hel");
        assert_eq!(f.read(r, 10).unwrap(), b"lo!");
        assert_eq!(f.read(r, 10).unwrap(), b"");
        assert_eq!(f.open("x", 2), Err("File name x has unknown requested opening flag".into()));
        assert!(f.open("../x", O_WRONLY).is_err());
        assert!(f.open("/etc/passwd", O_RDONLY).is_err());
        assert!(f.write(1, b"x").is_err(), "the console is not a file here");
    }

    #[test]
    fn quota_and_descriptors() {
        let mut f = Files::new(Storage::Memory(BTreeMap::new()));
        let w = f.open("big", O_WRONLY).unwrap();
        let chunk = vec![0; 1 << 20];
        for _ in 0..16 {
            f.write(w, &chunk).unwrap();
        }
        assert!(f.write(w, b"x").unwrap_err().contains("more than 16 MB"));
        f.close(w);
        for i in 0..MAX_FILES - 3 {
            f.open(&format!("f{i}"), O_WRONLY).unwrap();
        }
        assert!(f.open("one more", O_WRONLY).unwrap_err().contains("maximum open file limit of 32"));
    }

    #[test]
    fn host_folder_is_a_sandbox() {
        let root = std::env::temp_dir().join(format!("rustemu_mips_files_{}", std::process::id()));
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let mut f = Files::new(Storage::Host(root.clone()));
        let w = f.open("sub/out.txt", O_WRONLY).unwrap();
        f.write(w, b"data").unwrap();
        f.close(w);
        assert_eq!(std::fs::read(root.join("sub/out.txt")).unwrap(), b"data");
        assert!(f.open("../outside.txt", O_WRONLY).is_err());
        assert!(f.open("missing/x.txt", O_WRONLY).is_err());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(std::env::temp_dir(), root.join("link")).unwrap();
            assert!(f.open("link/escape.txt", O_WRONLY).is_err());
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
