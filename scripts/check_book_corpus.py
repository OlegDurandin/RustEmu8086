#!/usr/bin/env python3
"""Audit chapters 01–12 without modifying the archive; Python standard library only."""
import argparse
import concurrent.futures
import hashlib
import html
import json
import re
import shutil
import subprocess
import time
from collections import Counter
from pathlib import Path


def command(argv, cwd, log, timeout=10):
    start = time.monotonic()
    try:
        p = subprocess.run([str(x) for x in argv], cwd=cwd, stdin=subprocess.DEVNULL,
                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)
        result = {"code": p.returncode, "stdout": p.stdout.decode("utf-8", "replace"),
                  "stderr": p.stderr.decode("utf-8", "replace"), "timeout": False}
    except subprocess.TimeoutExpired as e:
        result = {"code": None, "stdout": (e.stdout or b"").decode("utf-8", "replace"),
                  "stderr": (e.stderr or b"").decode("utf-8", "replace"), "timeout": True}
    result["seconds"] = round(time.monotonic() - start, 3)
    log.parent.mkdir(parents=True, exist_ok=True)
    log.write_text("COMMAND: " + repr([str(x) for x in argv]) + "\n\nSTDOUT\n" +
                   result["stdout"] + "\nSTDERR\n" + result["stderr"], encoding="utf-8")
    result["log"] = str(log)
    return result


def runtime_status(r):
    err = r["stderr"]
    if r["timeout"]:
        return "timeout"
    if "[input exhausted]" in err:
        return "needs_input"
    if "[instruction limit reached]" in err:
        return "instruction_limit"
    if "[error at " in err:
        return "cpu_error"
    if "[halted at " in err:
        return "halted"
    if "error:" in err or r["code"] is None or r["code"] < 0:
        return "load_error"
    return "terminated"  # DOS may return any exit code, including nonzero.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("source", type=Path)
    parser.add_argument("--out", type=Path, default=Path("target/book-corpus"))
    parser.add_argument("--emulator", type=Path, default=Path("target/release/rustemu"))
    parser.add_argument("--max", type=int, default=200000)
    args = parser.parse_args()
    root, out, emu = args.source.resolve(), args.out.resolve(), args.emulator.resolve()
    if out.exists():
        raise SystemExit("Output already exists; choose a new --out directory")
    out.mkdir(parents=True)
    # On macOS the first launch of a rebuilt executable may wait for code verification.
    # Complete that launch before starting short per-case deadlines in parallel.
    warmup=command([emu,"--help"],out,out/"emulator-startup.log",timeout=60)
    if warmup["code"]!=0:
        raise SystemExit("Emulator could not start; see emulator-startup.log")
    inputs = out / "archive-copy"
    for chapter in range(1, 13):
        shutil.copytree(root / f"{chapter:02}", inputs / f"{chapter:02}")
    files = sorted(p for p in inputs.rglob("*") if p.is_file())
    listings = [p for p in files if p.suffix.lower() in (".8", ".08", ".8x", ".asm")]
    includes = [p for p in files if p.suffix.lower() == ".inc"]
    quiz_origins = {}
    for p in files:
        if not re.fullmatch(r"quiz\d+\.txt", p.name, re.I):
            continue
        text = p.read_text(encoding="utf-8")
        boundaries = list(re.finditer(r"(?m)^\s*;\s*-+\s*(\d+)\s*-+[^\n]*", text))
        for i, match in enumerate(boundaries):
            end = boundaries[i + 1].start() if i + 1 < len(boundaries) else len(text)
            snippet = p.with_name(p.name + ".part-" + match[1].zfill(2) + ".8")
            snippet.write_text(text[match.end():end], encoding="utf-8")
            quiz_origins[str(snippet.relative_to(inputs))] = {
                "file": str(p.relative_to(inputs)), "exercise": int(match[1]),
                "start_line": text[:match.end()].count("\n") + 1}
            listings.append(snippet)
    nasm = shutil.which("nasm")

    def check(p):
        rel = p.relative_to(inputs)
        case = out / "cases" / rel
        case.mkdir(parents=True)
        text = p.read_text(encoding="utf-8", errors="replace")
        record = {"file": str(rel), "role": "include" if p in includes else "listing",
                  "sha256": hashlib.sha256(p.read_bytes()).hexdigest(), "assembly": {}, "runs": {},
                  "uses_32bit_registers": bool(re.search(r"\b(?:e[abcd]x|e[sd]i|e[bs]p|[ao]32)\b", text, re.I))}
        if str(rel) in quiz_origins:
            record["quiz_origin"] = quiz_origins[str(rel)]
        # Structural hints only; no claim of correctness for an arbitrary fragment.
        active = "\n".join(line.split(";", 1)[0] for line in text.splitlines())
        record["has_explicit_stop"] = bool(re.search(
            r"\b(?:int\s+(?:0x20|0?20h|020)|hlt|ret[f]?|_exit)\b", active, re.I))
        record["has_a86_octal_looking_hex"] = bool(re.search(r"\b0[0-9a-f]{2,}\b", active, re.I))
        # Independent copies for DOS file services; no test can change another test's inputs.
        for dialect in ("auto", "nasm", "masm", "a86"):
            binary = case / (dialect + ".image")
            cmd = [emu, "asm", p, "-o", binary]
            if dialect != "auto":
                cmd += ["--syntax", dialect]
            r = command(cmd, p.parent, case / (dialect + ".asm.log"))
            record["assembly"][dialect] = r
            if r["code"] != 0 or record["role"] == "include":
                continue
            fmt = re.search(r", (com|exe|bin|boot) format,", r["stdout"], re.I)
            if not fmt:
                raise RuntimeError("Cannot determine output format: " + r["stdout"])
            run_dir = case / (dialect + "-runtime")
            shutil.copytree(p.parent, run_dir)
            # Keep source-relative includes and original extension for Auto, precisely as GUI does.
            if dialect == "auto":
                entry = run_dir / p.name
            else:
                entry = run_dir / ("__assembled." + fmt[1].lower())
                shutil.copyfile(binary, entry)
            run = command([emu, "run", entry, "--max", str(args.max), "--screen", "--regs"],
                          run_dir, case / (dialect + ".run.log"), timeout=5)
            run["status"] = runtime_status(run)
            record["runs"][dialect] = run
        if nasm:
            r = command([nasm, "-f", "bin", "-o", case / "reference.bin", p.name],
                        p.parent, case / "reference.asm.log")
            record["assembly"]["reference_nasm"] = r
            if r["code"] == 0 and record["assembly"]["nasm"]["code"] == 0:
                record["nasm_bytes_equal"] = (case / "reference.bin").read_bytes() == (case / "nasm.image").read_bytes()
            if r["code"] == 0 and record["role"] != "include":
                run_dir = case / "reference-runtime"
                shutil.copytree(p.parent, run_dir)
                # Match the emulator's flat NASM load convention; preserve explicit COM origin.
                com = bool(re.search(r"\borg\s+(?:0x100|100h|256)\b", active, re.I))
                entry = run_dir / ("__reference.com" if com else "__reference.bin")
                shutil.copyfile(case / "reference.bin", entry)
                run = command([emu, "run", entry, "--max", str(args.max), "--screen", "--regs"],
                              run_dir, case / "reference.run.log", timeout=5)
                run["status"] = runtime_status(run)
                run["load_format"] = "com" if com else "bin at 0100:0000"
                record["runs"]["reference_nasm"] = run
        return record

    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        for record in pool.map(check, listings + includes):
            results.append(record)
            print(record["file"], {k: v["code"] for k, v in record["assembly"].items()},
                  {k: v["status"] for k, v in record["runs"].items()}, flush=True)
    # Also run the author's prebuilt COM/EXE files, separately from source compatibility.
    binaries = []
    for p in files:
        if p.suffix.lower() not in (".com", ".exe"):
            continue
        rel = p.relative_to(inputs)
        case = out / "prebuilt" / rel
        shutil.copytree(p.parent, case)
        run = command([emu, "run", case / p.name, "--max", str(args.max), "--screen", "--regs"],
                      case, case / "run.log", timeout=5)
        run["status"] = runtime_status(run)
        binaries.append({"file": str(rel), "run": run})
    revision = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    branch = subprocess.run(["git", "branch", "--show-current"], capture_output=True, text=True).stdout.strip()
    dirty = subprocess.run(["git", "diff", "--quiet"], capture_output=True).returncode != 0
    data = {"source": str(root), "revision": revision,
            "branch": branch, "working_tree_dirty": dirty,
            "emulator_sha256": hashlib.sha256(emu.read_bytes()).hexdigest(), "instruction_limit": args.max,
            "startup_seconds":warmup["seconds"],
            "stdin": "EOF; no prepared input", "timer_irq": False,
            "nasm_version": subprocess.run([nasm, "-v"], capture_output=True, text=True).stdout.strip() if nasm else None,
            "quiz_fragments": len(quiz_origins), "results": results, "prebuilt": binaries}
    (out / "results.json").write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
    lines = ["Book chapters 01–12 — original source compatibility", "", f"Revision: {revision}; branch: {branch}; dirty: {dirty}",
             f"Listings: {len(listings)}; includes: {len(includes)}; prebuilt programs: {len(binaries)}",
             f"Limit: {args.max} instructions / 5 seconds per execution; stdin EOF; timer IRQ off.",
             "No source adaptation. Success means completion of execution, not verified algorithmic correctness.", ""]
    for dialect in ("auto", "nasm", "masm", "a86", "reference_nasm"):
        source_results = [r for r in results if r["role"] == "listing"]
        assembled = sum(r["assembly"].get(dialect, {}).get("code") == 0 for r in source_results)
        statuses = Counter(r["runs"][dialect]["status"] for r in source_results if dialect in r["runs"])
        lines.append(f"{dialect}: assembly {assembled}/{len(listings)}; runtime {dict(statuses)}")
    lines += ["", "All files:"]
    rows = []
    for r in results:
        cells = [r["file"], r["role"]]
        for dialect in ("auto", "nasm", "masm", "a86", "reference_nasm"):
            a = r["assembly"].get(dialect, {})
            status = "assembled" if a.get("code") == 0 else "assembly_error"
            if dialect in r["runs"]:
                status += "; " + r["runs"][dialect]["status"]
            cells.append(status)
        lines.append(" | ".join(cells))
        rows.append("<tr>" + "".join("<td>" + html.escape(c) + "</td>" for c in cells) + "</tr>")
    (out / "report.txt").write_text("\n".join(lines) + "\n", encoding="utf-8")
    page = ("<!doctype html><meta charset=utf-8><title>Book corpus 01–12</title>"
            "<style>body{font:14px system-ui;margin:32px}td,th{padding:8px;border:1px solid #ccc}"
            "table{border-collapse:collapse}pre{white-space:pre-wrap}</style><h1>Book corpus 01–12</h1><pre>" +
            html.escape("\n".join(lines[:12])) + "</pre><table><tr>" +
            "".join("<th>" + h + "</th>" for h in ["File", "Role", "Auto", "NASM", "MASM", "A86", "Reference NASM"]) +
            "</tr>" + "".join(rows) + "</table><p>Full diagnostics: results.json and cases/*/*.log</p>")
    (out / "index.html").write_text(page, encoding="utf-8")
    print("\n".join(lines[:12]), flush=True)
    print("REPORT:", out, flush=True)


if __name__ == "__main__":
    main()
