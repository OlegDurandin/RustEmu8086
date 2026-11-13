//! Owned build requests and results shared by desktop tasks and a dedicated browser worker.
//! Filesystem access belongs to the task. The UI only submits snapshots and applies results.

#![warn(missing_docs)]
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub mod mars;
pub mod project;

/// Snapshot of one document revision and its build configuration.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Request {
    /// Document revision; echoed unchanged in the result.
    pub version: u64,
    /// MIPS32 when true, 8086 otherwise.
    pub mips: bool,
    /// Main source path or logical name.
    pub name: String,
    /// Main source snapshot.
    pub text: String,
    /// Explicit 8086 dialect; None means detection.
    pub dialect: Option<rustemu_asm::Dialect>,
    /// Settings affecting MIPS assembly.
    pub mars: rustemu_mips::Settings,
    /// Snapshot of browser files and built-in include files.
    pub files: BTreeMap<String, String>,
    /// Native include fallback roots (ignored when host is false).
    pub roots: Vec<PathBuf>,
    /// Native filesystem access; browser workers always use false.
    pub host: bool,
    /// Restrict lookups to exact normalized package paths.
    #[serde(default)]
    pub portable: bool,
    /// Optional native exception-handler path.
    pub exception_handler: Option<String>,
    /// Assemble the other .asm/.s files in the main source directory.
    pub project: bool,
}

/// Typed assembler result; no machine, handles or UI objects cross the task boundary.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Outcome {
    /// 8086 result and source diagnostics.
    X86(Result<rustemu_asm::Program, rustemu_asm::AsmErrors>),
    /// MIPS result and source diagnostics.
    Mips(Result<rustemu_mips::asm::Program, rustemu_mips::asm::AsmErrors>),
    /// Cancelled, failed loader, worker or internal error.
    Failed(String),
}

/// Completed request, with exact source snapshots used by the debugger.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Response {
    /// Submitted revision.
    pub version: u64,
    /// Build output.
    pub outcome: Outcome,
    /// Sources actually read, by their resolved names.
    pub sources: BTreeMap<String, String>,
    /// Source bytes charged to this build.
    pub source_bytes: usize,
    /// Number of charged source units, including repeated includes.
    pub source_files: usize,
    /// Charged physical and generated lines (physical snapshots only on a failed build).
    pub source_lines: usize,
}

struct Loader<'a> {
    request: &'a Request,
    sources: BTreeMap<String, String>,
    used: usize,
    failure: Option<String>,
    cancel: Arc<AtomicBool>,
}

impl Loader<'_> {
    fn read(&mut self, name: &str) -> Option<String> {
        use rustemu_mips::asm::{MAX_SOURCE_BYTES, MAX_SOURCE_FILES};
        if self.cancel.load(Ordering::Relaxed) {
            self.failure = Some("Assembly cancelled".into());
            return None;
        }
        if let Some(s) = self.sources.get(name) {
            return Some(s.clone());
        }
        if self.sources.len() >= MAX_SOURCE_FILES {
            self.failure = Some("Source file budget exceeded".into());
            return None;
        }
        if name.len() > rustemu_mips::asm::MAX_SOURCE_PATH_BYTES {
            self.failure = Some("Source path budget exceeded".into());
            return None;
        }
        let normalized;
        let name = if self.request.portable {
            normalized = project::source_path(name)?;
            normalized.as_str()
        } else {
            name
        };
        let base = Path::new(name).file_name()?.to_string_lossy();
        let from_memory = self
            .request
            .files
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name) || (!self.request.portable && k.eq_ignore_ascii_case(&base)))
            .map(|(_, v)| v.clone());
        let source = if self.request.host {
            std::iter::once(PathBuf::from(name))
                .chain(self.request.roots.iter().flat_map(|d| [d.join(name), d.join(base.as_ref())]))
                .find_map(|p| rustemu_mips::asm::read_source_file(p).ok())
                .or(from_memory)
        } else {
            from_memory
        }?;
        if source.len() > MAX_SOURCE_BYTES.saturating_sub(self.used) {
            self.failure = Some("Source byte budget exceeded".into());
            return None;
        }
        self.used += source.len();
        self.sources.insert(name.into(), source.clone());
        Some(source)
    }
}
impl rustemu_asm::FileLoader for Loader<'_> {
    fn load(&mut self, name: &str, from_file: &str) -> Option<(String, String)> {
        if name.eq_ignore_ascii_case("emu8086.inc") && !self.request.portable {
            let key = "emu8086.inc";
            if !self.sources.contains_key(key) {
                let s = rustemu_asm::EMU8086_INC.to_string();
                if s.len() > rustemu_mips::asm::MAX_SOURCE_BYTES.saturating_sub(self.used) {
                    self.failure = Some("Source byte budget exceeded".into());
                    return None;
                }
                self.used += s.len();
                self.sources.insert(key.into(), s);
            }
            return Some((key.into(), self.sources[key].clone()));
        }
        let resolved = Path::new(from_file).parent().unwrap_or(Path::new("")).join(name).to_string_lossy().into_owned();
        self.read(&resolved).map(|s| (resolved, s))
    }
}

/// Compiles a bounded request on a task/worker. Panics become a failed result on native builds;
/// a browser trap terminates its worker and is reported through the worker error event.
pub fn execute(request: Request, cancel: Arc<AtomicBool>) -> Response {
    let mut loader = Loader {
        used: request.text.len(),
        sources: BTreeMap::new(),
        request: &request,
        failure: None,
        cancel: cancel.clone(),
    };
    let outcome = if request.name.len() > rustemu_mips::asm::MAX_SOURCE_PATH_BYTES {
        Outcome::Failed("Source path budget exceeded".into())
    } else if request.text.len() > rustemu_mips::asm::MAX_SOURCE_BYTES {
        Outcome::Failed("Source byte budget exceeded".into())
    } else if cancel.load(Ordering::Relaxed) {
        Outcome::Failed("Assembly cancelled".into())
    } else {
        loader.sources.insert(request.name.clone(), request.text.clone());
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if !request.mips {
                return Outcome::X86(rustemu_asm::assemble_cancellable(
                    &request.name,
                    &request.text,
                    &mut loader,
                    request.dialect,
                    cancel.clone(),
                ));
            }
            let read_unit = |loader: &mut Loader<'_>, p: &str| {
                loader.read(p).map(|text| rustemu_mips::asm::SourceFile { name: p.into(), text })
            };
            let handler = match request.exception_handler.as_deref() {
                Some(p) if request.host || request.portable => match read_unit(&mut loader, p) {
                    Some(h) => Some(h),
                    None => return Outcome::Failed(format!("Cannot read exception handler: {p}")),
                },
                _ => None,
            };
            let mut others = Vec::new();
            if request.project && request.host {
                let dir =
                    Path::new(&request.name).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
                let entries = match std::fs::read_dir(dir) {
                    Ok(e) => e,
                    Err(e) => return Outcome::Failed(e.to_string()),
                };
                let main = Path::new(&request.name).canonicalize().ok();
                let mut paths: Vec<_> = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| {
                        p.is_file()
                            && p.canonicalize().ok() != main
                            && p.extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("asm") || e.eq_ignore_ascii_case("s"))
                    })
                    .take(rustemu_mips::asm::MAX_SOURCE_FILES)
                    .collect();
                if paths.len() + 1 + usize::from(handler.is_some()) > rustemu_mips::asm::MAX_SOURCE_FILES {
                    return Outcome::Failed("Source file budget exceeded".into());
                }
                paths.sort_by_key(|p| (!p.extension().is_some_and(|e| e.eq_ignore_ascii_case("asm")), p.clone()));
                for path in paths {
                    let name = path.to_string_lossy();
                    let Some(f) = read_unit(&mut loader, &name) else {
                        return Outcome::Failed(format!("Cannot read source: {name}"));
                    };
                    others.push(f);
                }
            }
            if request.project && !request.host && request.portable {
                let parent = Path::new(&request.name).parent();
                let mut names: Vec<_> = request
                    .files
                    .keys()
                    .filter(|name| {
                        *name != &request.name
                            && Some(name.as_str()) != request.exception_handler.as_deref()
                            && Path::new(name).parent() == parent
                            && Path::new(name)
                                .extension()
                                .is_some_and(|e| e.eq_ignore_ascii_case("asm") || e.eq_ignore_ascii_case("s"))
                    })
                    .cloned()
                    .collect();
                names.sort_by_key(|p| (!p.to_ascii_lowercase().ends_with(".asm"), p.clone()));
                for name in names {
                    let Some(f) = read_unit(&mut loader, &name) else {
                        return Outcome::Failed(format!("Cannot read source: {name}"));
                    };
                    others.push(f);
                }
            }
            Outcome::Mips(rustemu_mips::asm::assemble_files_cancellable(
                rustemu_mips::asm::SourceFile { name: request.name.clone(), text: request.text.clone() },
                handler,
                others,
                &mut |name| loader.read(name),
                &request.mars,
                cancel.clone(),
            ))
        }))
        .unwrap_or_else(|_| Outcome::Failed("Internal error of the assembler".into()))
    };
    let outcome = loader.failure.clone().map_or(outcome, Outcome::Failed);
    let (source_bytes, source_lines, source_files) = match &outcome {
        Outcome::X86(Ok(p)) => (p.source_bytes, p.processed_lines, p.files.len()),
        Outcome::Mips(Ok(p)) => (p.source_bytes, p.processed_lines, p.files.len()),
        _ => (loader.used, loader.sources.values().map(|s| s.lines().count()).sum(), loader.sources.len()),
    };
    Response { version: request.version, outcome, source_bytes, source_lines, source_files, sources: loader.sources }
}

/// Serializes a response with a hard allocation bound, including JSON's metadata/escaping overhead.
pub fn serialize_response(response: &Response) -> Result<String, String> {
    use rustemu_target::resources::{Resource, ResourceBudget};
    struct Bounded(Vec<u8>);
    impl std::io::Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            ResourceBudget::check(Resource::BuildResultBytes, self.0.len(), bytes.len())
                .map_err(std::io::Error::other)?;
            self.0.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded(Vec::new());
    serde_json::to_writer(&mut output, response).map_err(|e| format!("Assembly result transfer failed: {e}"))?;
    String::from_utf8(output.0).map_err(|e| e.to_string())
}
