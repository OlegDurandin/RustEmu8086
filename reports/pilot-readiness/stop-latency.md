# A Stop latency

BLOCKED: real browser control unavailable. No measurements; no median, p95 or max claimed. 100 ms is a target only.

Native GUI follow-up: one 8086 INC/JMP infinite loop was stopped through the real Stop button; Running → Paused observed with preserved register state. See [desktop report](desktop-gui.md), screenshots 18/19. This adds a functional observation only; none of the browser timing samples below is counted as completed.

Run 10 repetitions EACH: empty infinite loop; REP MOVSW with CX=FFFF; intensive output; MMIO/robot; checking an infinite solution. Record device, OS, browser, SHA, zoom and background load. Measure pointerdown on Stop/Cancel to the first frame confirming stopped state using a browser performance trace or timestamped high-rate video. Keep all raw samples. For 10 samples median is the average of the 5th and 6th sorted samples; p95 uses nearest rank ceil(.95*N); max is largest.

| Scenario | Repetitions | median/p95/max | Status |
|---|---|---|---|
| Loop | 0/10 | NOT MEASURED | BLOCKED |
| REP | 0/10 | NOT MEASURED | BLOCKED |
| Output | 0/10 | NOT MEASURED | BLOCKED |
| MMIO | 0/10 | NOT MEASURED | BLOCKED |
| Check limit | 0/10 | NOT MEASURED | BLOCKED |

CheckRun yields after 1024 dispatches or a 4 ms check. This is not measured Stop latency; one expensive instruction cannot be interrupted mid-instruction. Hardware confirmation remains required before GO.
