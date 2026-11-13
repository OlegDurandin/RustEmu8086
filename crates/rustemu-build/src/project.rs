//! Versioned, bounded portable projects shared by front ends. No host paths are restored.
use crate::{Outcome, Request, mars::MarsSettings};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, atomic::AtomicBool},
};

/// Maximum JSON file size, including escaped source text.
pub const MAX_PACKAGE_BYTES: usize = 32 << 20;
/// Maximum prepared console input size.
pub const MAX_INPUT_BYTES: usize = 4096;

/// A self-contained source project (`.remu`, JSON, version 1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    /// Format version; unsupported versions are rejected before opening.
    pub version: u32,
    /// `8086` or `MIPS32`.
    pub architecture: String,
    /// Main source path within files.
    pub entry: String,
    /// Explicit x86 dialect, or detection.
    pub dialect: Option<rustemu_asm::Dialect>,
    /// Assembly and execution settings, including arguments and handler path.
    pub mars: MarsSettings,
    /// UTF-8 sources and include files indexed by relative POSIX paths.
    pub files: BTreeMap<String, String>,
    /// Prepared console input; LF is Enter. Reapplied on load/reload.
    pub input: String,
}

/// Normalizes an internal source reference without allowing escape from the package.
pub fn source_path(name: &str) -> Option<String> {
    if name.starts_with('/') || name.contains(['\\', ':', '\0']) {
        return None;
    }
    let mut parts = Vec::new();
    for p in name.split('/') {
        match p {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(p),
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

impl Project {
    /// Validates version, configuration, inventory and allocation budgets.
    pub fn validate(&self) -> Result<(), String> {
        use rustemu_mips::asm::{MAX_SOURCE_BYTES, MAX_SOURCE_FILES, MAX_SOURCE_PATH_BYTES};
        if self.version != 1 {
            return Err(format!("Unsupported project version: {}", self.version));
        }
        if !matches!(self.architecture.as_str(), "8086" | "MIPS32") {
            return Err("Unknown project architecture".into());
        }
        if self.files.is_empty() || self.files.len() > MAX_SOURCE_FILES {
            return Err("Project file budget exceeded".into());
        }
        let mut total = 0usize;
        let mut names = std::collections::BTreeSet::new();
        for (name, text) in &self.files {
            if name.len() > MAX_SOURCE_PATH_BYTES || source_path(name).as_deref() != Some(name) {
                return Err(format!("Invalid project path: {name}"));
            }
            if !names.insert(name.to_lowercase()) {
                return Err("Ambiguous project file names".into());
            }
            total = total.saturating_add(text.len());
            if total > MAX_SOURCE_BYTES {
                return Err("Project source byte budget exceeded".into());
            }
        }
        if !self.files.contains_key(&self.entry) {
            return Err("Project entry file is missing".into());
        }
        if self.input.len() > MAX_INPUT_BYTES {
            return Err("Project input budget exceeded".into());
        }
        if rustemu_mips::MemoryConfig::from_id(&self.mars.memory).is_none() {
            return Err("Unknown MARS memory configuration".into());
        }
        if self.mars.exception_handler_enabled && !self.files.contains_key(&self.mars.exception_handler) {
            return Err("Project exception handler is missing".into());
        }
        Ok(())
    }

    /// Reads a bounded JSON package. Never writes files or touches a machine.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("Project package byte budget exceeded".into());
        }
        let project: Self = serde_json::from_slice(bytes).map_err(|e| format!("Invalid project: {e}"))?;
        project.validate()?;
        Ok(project)
    }

    /// Writes the same package used in files and compressed links.
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > MAX_PACKAGE_BYTES {
            return Err("Project package byte budget exceeded".into());
        }
        Ok(bytes)
    }

    /// Makes an assembly request restricted to packaged files.
    pub fn request(&self, version: u64) -> Request {
        Request {
            version,
            mips: self.architecture == "MIPS32",
            name: self.entry.clone(),
            text: self.files[&self.entry].clone(),
            dialect: self.dialect,
            mars: self.mars.mips(),
            files: self.files.clone(),
            roots: vec![],
            host: false,
            portable: true,
            exception_handler: self.mars.exception_handler_enabled.then(|| self.mars.exception_handler.clone()),
            project: self.mars.assemble_all,
        }
    }

    /// Captures the exact sources resolved by a successful assembly, removing the main folder
    /// prefix. External/absolute include references must first be moved into the source folder.
    pub fn capture(request: Request, mut mars: MarsSettings, input: String) -> Result<Self, String> {
        let response = crate::execute(request.clone(), Arc::new(AtomicBool::new(false)));
        match response.outcome {
            Outcome::X86(Ok(_)) | Outcome::Mips(Ok(_)) => {}
            Outcome::Failed(e) => return Err(e),
            _ => return Err("Cannot package project: fix assembly errors first".into()),
        }
        let root = Path::new(&request.name).parent().unwrap_or(Path::new(""));
        let relative = |name: &str| -> Result<String, String> {
            let path = Path::new(name).strip_prefix(root).unwrap_or(Path::new(name));
            let name = path.to_string_lossy();
            source_path(&name).ok_or_else(|| format!("Move dependency into the project folder: {name}"))
        };
        let mut files = BTreeMap::new();
        for (name, text) in response.sources {
            let key = relative(&name)?;
            if files.insert(key, text).is_some() {
                return Err("Conflicting project paths".into());
            }
        }
        if let Some(handler) = request.exception_handler {
            mars.exception_handler = relative(&handler)?;
        } else {
            mars.exception_handler.clear();
        }
        let p = Self {
            version: 1,
            architecture: if request.mips { "MIPS32" } else { "8086" }.into(),
            entry: relative(&request.name)?,
            dialect: request.dialect,
            mars,
            files,
            input,
        };
        p.validate()?;
        // Rebuild without filesystem fallbacks: refuse an apparently complete but unusable export.
        match crate::execute(p.request(0), Arc::new(AtomicBool::new(false))).outcome {
            Outcome::X86(Ok(_)) | Outcome::Mips(Ok(_)) => Ok(p),
            _ => Err("Project uses dependencies outside its folder; use relative includes inside the project".into()),
        }
    }
}

/// Reads a native package with a hard bound before JSON parsing.
pub fn read_package(path: &Path) -> Result<Project, String> {
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((MAX_PACKAGE_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Project::decode(&bytes)
}
