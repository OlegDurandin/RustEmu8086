//! Shared resource policy and usage reports. Counters belong to their subsystem, never to UI.

use std::collections::HashMap;

/// A resource with an explicit owner and unit of accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Resource {
    /// Retained reversible state, bytes.
    HistoryBytes,
    /// Retained instructions.
    HistorySteps,
    /// Retained program output, bytes.
    OutputBytes,
    /// Retained device output, bytes.
    DeviceBytes,
    /// Bytes written to files during one run.
    FileWriteBytes,
    /// New host files/folders per run.
    CreatedFiles,
    /// Bytes on the shared memory drive.
    DriveBytes,
    /// Files and directories on the shared memory drive.
    DriveEntries,
    /// Independent MIPS random streams.
    RandomStreams,
    /// Physical input text, UTF-8 bytes per assembly.
    SourceBytes,
    /// Loaded sources per assembly.
    SourceFiles,
    /// Physical and generated lines per assembly.
    SourceLines,
    /// Generated text, UTF-8 bytes per assembly.
    ExpandedBytes,
    /// One line, UTF-8 bytes.
    LineBytes,
    /// UTF-8 bytes in one source path.
    SourcePathBytes,
    /// Tokens retained on one MIPS line.
    LineTokens,
    /// Tokens processed per MIPS assembly.
    ProcessedTokens,
    /// Serialized browser build response bytes.
    BuildResultBytes,
    /// Elapsed assembly time in milliseconds.
    BuildTimeMs,
    /// Diagnostic messages per assembly.
    Diagnostics,
    /// Bytes shown in one step preview.
    StepPreview,
}

/// What a quota owner does on exhaustion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exhaustion {
    /// Return an error without committing the rejected change.
    Error,
    /// Evict the oldest retained records.
    EvictOldest,
    /// Keep the newest output and expose a dropped-byte count.
    DropOldest,
    /// Show a bounded prefix and expose the total count.
    Preview,
}

/// One live resource counter, together with the policy that enforces it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceUsage {
    /// Counter identity.
    pub resource: Resource,
    /// Owner of the quota (machine, files, assembler, device or preview).
    pub owner: &'static str,
    /// Bytes, steps, files, entries, streams, lines or messages.
    pub unit: &'static str,
    /// Currently charged units; cumulative for per-run quotas.
    pub used: usize,
    /// Maximum charged units.
    pub limit: usize,
    /// Result of attempting to exceed the maximum.
    pub exhaustion: Exhaustion,
}

/// Shared policy and a snapshot of subsystem-owned counters.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ResourceBudget {
    /// Live counters. Owners may expose only counters relevant to their architecture.
    pub usage: Vec<ResourceUsage>,
}

impl ResourceBudget {
    /// Default maximum; all enforcing owners reference these values.
    pub const fn limit(resource: Resource) -> usize {
        match resource {
            Resource::HistoryBytes => 64 << 20,
            Resource::HistorySteps => 2000,
            Resource::OutputBytes | Resource::DeviceBytes => 1 << 20,
            Resource::FileWriteBytes => 16 << 20,
            Resource::CreatedFiles | Resource::DriveEntries | Resource::SourceFiles => 1024,
            Resource::DriveBytes => 2 << 20,
            Resource::RandomStreams => 4096,
            Resource::SourceBytes => 4 << 20,
            Resource::SourceLines => 250_000,
            Resource::ExpandedBytes => 16 << 20,
            Resource::LineBytes => 64 << 10,
            Resource::Diagnostics => 100,
            Resource::BuildResultBytes => 64 << 20,
            Resource::BuildTimeMs => 120_000,
            Resource::SourcePathBytes => 4096,
            Resource::LineTokens => 4096,
            Resource::ProcessedTokens => 1_000_000,
            Resource::StepPreview => 256,
        }
    }

    /// Checks an addition without overflowing or changing any state.
    pub fn check(resource: Resource, used: usize, addition: usize) -> Result<(), String> {
        let limit = Self::limit(resource);
        if addition > limit.saturating_sub(used) || used > limit {
            Err(format!("{resource:?} limit exceeded ({limit})"))
        } else {
            Ok(())
        }
    }

    /// Appends one counter to a report.
    pub fn record(&mut self, resource: Resource, used: usize) {
        use Exhaustion::*;
        let (owner, unit, exhaustion) = match resource {
            Resource::HistoryBytes => ("machine history", "bytes", EvictOldest),
            Resource::HistorySteps => ("machine history", "steps", EvictOldest),
            Resource::OutputBytes => ("program console", "bytes", DropOldest),
            Resource::DeviceBytes => ("devices", "bytes", DropOldest),
            Resource::FileWriteBytes => ("file layer (per run)", "bytes", Error),
            Resource::CreatedFiles => ("file layer (per run)", "files", Error),
            Resource::DriveBytes => ("memory drive", "bytes", Error),
            Resource::DriveEntries => ("memory drive", "entries", Error),
            Resource::RandomStreams => ("MIPS machine", "streams", Error),
            Resource::SourceBytes | Resource::ExpandedBytes | Resource::LineBytes => ("assembler", "bytes", Error),
            Resource::SourceFiles => ("assembler", "files", Error),
            Resource::SourceLines => ("assembler", "lines", Error),
            Resource::Diagnostics => ("assembler", "messages", Error),
            Resource::BuildResultBytes => ("build worker", "bytes", Error),
            Resource::BuildTimeMs => ("build task", "ms", Error),
            Resource::SourcePathBytes => ("assembler", "bytes", Error),
            Resource::LineTokens | Resource::ProcessedTokens => ("MIPS assembler", "tokens", Error),
            Resource::StepPreview => ("step view", "bytes", Preview),
        };
        self.usage.push(ResourceUsage { resource, owner, unit, used, limit: Self::limit(resource), exhaustion });
    }
}

/// A bounded memory-change preview, retaining the first value written at each address.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryChanges {
    /// Address-sorted prefix of changed bytes: address, before, after.
    pub shown: Vec<(u32, u8, u8)>,
    /// Total unique addresses whose final value differs from the initial value.
    pub total: usize,
}

/// Deduplicates in expected linear time, reads each unique byte once, and bounds UI output.
pub fn memory_changes(undo: impl IntoIterator<Item = (u32, u8)>, mut read: impl FnMut(u32) -> u8) -> MemoryChanges {
    let mut first = HashMap::new();
    for (address, old) in undo {
        first.entry(address).or_insert(old);
    }
    let mut changed: Vec<_> = first
        .into_iter()
        .filter_map(|(a, b)| {
            let after = read(a);
            (after != b).then_some((a, b, after))
        })
        .collect();
    let total = changed.len();
    let limit = ResourceBudget::limit(Resource::StepPreview);
    if total > limit {
        changed.select_nth_unstable_by_key(limit, |v| v.0);
        changed.truncate(limit);
    }
    changed.sort_unstable_by_key(|v| v.0);
    MemoryChanges { shown: changed, total }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_reads_each_unique_address_once_and_keeps_initial_values() {
        let mut reads = 0;
        let undo = (0..131_070).map(|a| (a, 0)).chain([(0, 1), (1, 2)]);
        let result = memory_changes(undo, |_| {
            reads += 1;
            3
        });
        assert_eq!(reads, 131_070);
        assert_eq!(result.total, 131_070);
        assert_eq!(result.shown.len(), 256);
        assert_eq!(result.shown[0], (0, 0, 3));
        assert!(ResourceBudget::check(Resource::DriveBytes, usize::MAX, 1).is_err());
    }
}
