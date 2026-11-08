# Resource budgets and background assembly

`rustemu-target::resources::ResourceBudget` defines shared limits and reports. Each subsystem owns
its counter and checks mutations before allocating or committing them. `Target::resources()` reports
live machine counters; the UI adds the last build's source/generation counters. Open **Options →
Resource usage** in either layout. Counters describe charged payload/capacity, not process RSS.

| Owner / scope | Unit and default limit | Exhaustion |
|---|---|---|
| Machine history, both CPUs | 2,000 instructions and 64 MiB including dynamic snapshots | Evict oldest, even a single oversized record; execution continues |
| File layer, each run | 16 MiB written; 1,024 new host entries | DOS short write / error or MIPS syscall `-1`; existing files still open |
| Shared memory drive, persistent | 2 MiB contents; 1,024 files and directories combined | Reject before mutation; batch MIPS sync is atomic |
| MIPS file syscall | Read at most 64 KiB per call, write at most 16 MiB | Short read or controlled service error; validate before allocating |
| Console and textual devices | 1 MiB retained per buffer | Drop oldest text; console/printer expose dropped-byte count |
| MIPS random streams, machine | 4,096 stream IDs | Controlled syscall error; existing streams remain usable |
| Assembler, one build | 4 MiB source, 1,024 sources, 250,000 source/expansion lines, 64 KiB per line, 4 KiB per path | Diagnostic and stop; project/includes/handler share budgets |
| Generated text, one build | 16 MiB | Diagnostic and stop |
| MIPS tokenizer, one build | 4,096 tokens per line, 1,000,000 processed tokens | Diagnostic and stop |
| Assembler diagnostics | 100, including omission marker | Stop accumulating diagnostics |
| Step preview, either CPU | 256 changed bytes, with full changed-byte count | Show sorted prefix; deduplicate in expected linear time and cache until state changes |
| Browser result transfer | 64 MiB JSON | Worker error, no result applied |
| Build task | 120 seconds without completion | Cancel and report timeout |

History off disables snapshots, not file/source/output/RNG limits. Fixed device registers/maps do
not grow with guest execution; textual device buffers have their own bounds. Input supplied by the
user and process/runtime overhead are outside these guest-output quotas.

Desktop assembly runs on an owned background thread; browser assembly runs in a separate WASM
module Worker. Both use `rustemu-build` requests/results. Every request captures the source,
architecture, settings and document revision. Results for a changed document/configuration are
ignored. Debug source text comes from the completed request's exact snapshots.

**Cancel assembly**, Escape and Stop/F5 drop the pending task. Native assemblers poll an atomic
cancellation flag; the browser terminates its worker. A failed/cancelled build keeps the previous
machine available for inspection but cannot run it as the new document. Run and Step commands
continue after a successful background build. Headless `App::with_settings` keeps synchronous
assembly by default; interactive `App::new` enables tasks.

Execution currently remains cooperative in the UI. Options reports the last and maximum execution
tick time, so a single expensive instruction can be measured separately from assembly and preview
work before deciding how to isolate execution. The 12 ms loop target cannot preempt an instruction.
