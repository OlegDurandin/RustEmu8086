# Virtual devices

**English** · [Русский](Виртуальные-устройства)

As in Emu8086, simple devices are attached to the emulator. A program controls them through ports with `IN` and
`OUT`. The devices window is *aux* → *devices* or the **virtual devices** menu of the emulator window; a
`#start=name.exe#` directive at the top of a program opens the right tab right away.

Devices run in real time: the robot takes 250 ms per cell, and the thermometer heats up with the clock, not with
the instruction count.

| Device | `#start=` | Ports |
|---|---|---|
| Traffic lights | `traffic_lights.exe` | 4 (word) |
| Stepper motor | `stepper_motor.exe` | 7 |
| Robot | `robot.exe` | 9, 10, 11 |
| Thermometer and heater | `thermometer.exe` | 125, 127 |
| Printer | `printer.exe` | 130 |
| LED display | `led_display.exe` | 199 (word) |
| Simple I/O | `simple.exe` | 110 (byte), 112 (word) |
| VGA state | `vga_state.exe` | 3DAh, 3C8h / 3C9h |

Every device except simple I/O and VGA state has an example in **examples**.

## Traffic lights — port 4

A 12-bit word: four traffic lights (north, east, south, west), 3 bits each. For light k, bit 3k is red, 3k+1
yellow, 3k+2 green.

```asm
#start=traffic_lights.exe#
org 100h
    mov ax, 0000_0010_0100_1001b   ; red on all four lights
    out 4, ax
    ret
```

## LED display — port 199

Shows the written word as a signed decimal number.

```asm
#start=led_display.exe#
org 100h
    mov ax, 2026
    out 199, ax
    ret
```

## Printer — port 130

Every byte written is printed on the "paper". You can also print through DOS: INT 21h, function 05h, or by writing
to handle 4.

```asm
    mov al, 'A'
    out 130, al
```

## Thermometer and heater — ports 125 and 127

- `in al, 125` — the temperature in degrees Celsius (a signed byte, −65…120).
- `out 127, al` — bit 0 turns the heater on (1) or off (0).

With the heater on, the temperature rises 1° every 200 ms; with it off, it moves towards the air temperature at 1°
per second.

```asm
#start=thermometer.exe#
org 100h
check:
    in al, 125              ; temperature
    cmp al, 60
    jl heat
    mov al, 0               ; warm enough: heater off
    out 127, al
    jmp check
heat:
    mov al, 1               ; heater on
    out 127, al
    jmp check
```

## Stepper motor — port 7

- `out 7, al` — bits 0..2 energise the three windings. The motor reacts only when the value changes: by a half step
  or a full step, clockwise or counter-clockwise — as in Emu8086. One revolution is 32 half steps.
- `in al, 7` — bit 7 = 1: the motor is ready for the next command.

The half-step sequence clockwise: `110b, 100b, 011b, 010b`; counter-clockwise: `011b, 001b, 110b, 010b`. A full
example is `stepper_motor.asm` in **examples**.

## Robot — ports 9, 10, 11

The robot moves around a 9×6 room with walls and lamps. The map can be changed by clicking in the device window.

**Port 9 (OUT) — command**

| Code | Command |
|---|---|
| 0 | nothing |
| 1 | move forward |
| 2 | turn left |
| 3 | turn right |
| 4 | examine the cell in front |
| 5 | switch the lamp on |
| 6 | switch the lamp off |

**Port 10 (IN) — the examined cell:** 0 — empty, 255 — wall, 7 — lamp on, 8 — lamp off.

**Port 11 (IN) — status:** bit 0 — new data available (cleared by reading port 10), bit 1 — the robot is busy,
bit 2 — the last command failed (for example, a wall ahead).

A move or a turn takes 250 ms; a command sent while the robot is busy is rejected. So programs wait for bit 1 to
become 0 before a command:

```asm
#start=robot.exe#
org 100h
    mov al, 1               ; move forward
    out 9, al
wait:
    in al, 11               ; status
    test al, 2              ; bit 1: busy
    jnz wait
    mov al, 4               ; examine the cell in front
    out 9, al
    in al, 10               ; 0 empty, 255 wall, 7/8 lamp on/off
    ret
```

## Simple I/O — ports 110 and 112

The *simple I/O* tab shows the byte of port 110 and the word of port 112. The values can be changed in the
window — the program reads them with `IN`.

## VGA state

- Port 3DAh: the vertical retrace bit toggles by itself, so programs that wait for it work without any setup.
- Ports 3C8h / 3C9h: the DAC palette for mode 13h (an index, then three components 0..63).

The *VGA state* tab shows this bit, the video mode and the palette.

Any other port behaves as a latch: `IN` returns the last value written.
