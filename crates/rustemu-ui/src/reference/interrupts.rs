//! BIOS and DOS interrupts by function.

/// A BIOS / DOS interrupt.
pub struct Interrupt {
    pub number: u8,
    pub title: &'static str,
    pub description: &'static str,
    /// Register that selects the function (empty if none).
    pub selector: &'static str,
    pub functions: &'static [Function],
}

pub struct Function {
    pub code: u16,
    pub title: &'static str,
    pub input: &'static str,
    pub output: &'static str,
    pub example: &'static str,
}

macro_rules! func {
    ($code:literal, $title:literal, $input:literal, $output:literal, $example:literal) => {
        Function { code: $code, title: $title, input: $input, output: $output, example: $example }
    };
}

pub const INTERRUPTS: &[Interrupt] = &[
    Interrupt {
        number: 0x10,
        title: "Video services (BIOS)",
        description: "Screen output: video modes, cursor, characters with colours, scrolling and pixels. The text screen is 80x25 in mode 3 (memory at B800:0000, two bytes per character: code and attribute); mode 13h is 320x200 with 256 colours (memory at A000:0000).",
        selector: "AH",
        functions: &[
            func!(
                0x00,
                "Set video mode",
                "AL = mode: 00h/01h text 40x25, 02h/03h text 80x25 (16 colours, 8 pages), 13h graphics 320x200 256 colours",
                "The screen is cleared.",
                "mov ah, 0\nmov al, 3\nint 10h"
            ),
            func!(
                0x01,
                "Set cursor shape",
                "CH = first scan line (bit 5 = 1 hides the cursor), CL = last scan line",
                "",
                "mov ah, 1\nmov cx, 2607h   ; hide the cursor\nint 10h"
            ),
            func!(
                0x02,
                "Set cursor position",
                "DH = row, DL = column, BH = page",
                "",
                "mov ah, 2\nmov dh, 10\nmov dl, 20\nmov bh, 0\nint 10h"
            ),
            func!(
                0x03,
                "Get cursor position and shape",
                "BH = page",
                "DH = row, DL = column, CH/CL = cursor scan lines",
                "mov ah, 3\nmov bh, 0\nint 10h"
            ),
            func!(0x05, "Select active page", "AL = page (0..7)", "", "mov ah, 5\nmov al, 1\nint 10h"),
            func!(
                0x06,
                "Scroll window up",
                "AL = lines (0 = clear the window), BH = attribute for new lines, CH,CL = top row, column, DH,DL = bottom row, column",
                "",
                "mov ax, 0600h   ; clear\nmov bh, 07h\nmov cx, 0\nmov dx, 184Fh\nint 10h"
            ),
            func!(
                0x07,
                "Scroll window down",
                "Same registers as AH = 06h",
                "",
                "mov ax, 0701h\nmov bh, 07h\nmov cx, 0\nmov dx, 184Fh\nint 10h"
            ),
            func!(
                0x08,
                "Read character and attribute at the cursor",
                "BH = page",
                "AL = character, AH = attribute",
                "mov ah, 8\nmov bh, 0\nint 10h"
            ),
            func!(
                0x09,
                "Write character and attribute at the cursor",
                "AL = character, BL = attribute, BH = page, CX = repeat count",
                "The cursor does not move.",
                "mov ah, 9\nmov al, '*'\nmov bl, 1Eh     ; yellow on blue\nmov bh, 0\nmov cx, 10\nint 10h"
            ),
            func!(
                0x0A,
                "Write character at the cursor (keep the attribute)",
                "AL = character, BH = page, CX = count",
                "The cursor does not move.",
                "mov ah, 0Ah\nmov al, '-'\nmov bh, 0\nmov cx, 80\nint 10h"
            ),
            func!(
                0x0C,
                "Put pixel",
                "AL = colour, CX = column, DX = row (graphics modes)",
                "",
                "mov ah, 0Ch\nmov al, 15\nmov cx, 100\nmov dx, 50\nint 10h"
            ),
            func!(
                0x0D,
                "Get pixel",
                "CX = column, DX = row",
                "AL = colour",
                "mov ah, 0Dh\nmov cx, 100\nmov dx, 50\nint 10h"
            ),
            func!(
                0x0E,
                "Teletype output",
                "AL = character (BEL, BS, CR, LF are handled), BL = colour in graphics modes",
                "The cursor advances; the screen scrolls at the bottom.",
                "mov ah, 0Eh\nmov al, 'A'\nint 10h"
            ),
            func!(0x0F, "Get video mode", "", "AL = mode, AH = columns, BH = active page", "mov ah, 0Fh\nint 10h"),
            func!(
                0x10,
                "Palette / blink control (AL = 03h)",
                "AL = 03h, BL = 0: bit 7 of attributes means bright background (16 colours), BL = 1: blinking",
                "",
                "mov ax, 1003h\nmov bx, 0\nint 10h"
            ),
            func!(
                0x13,
                "Write string",
                "AL = mode (bit 0: move cursor, bit 1: string contains character/attribute pairs), BH = page, BL = attribute, CX = length, DH,DL = row, column, ES:BP = string",
                "",
                "mov ax, 1301h\nmov bx, 000Eh\nmov cx, 5\nmov dx, 0\nlea bp, txt     ; ES must point to the data\nint 10h"
            ),
        ],
    },
    Interrupt {
        number: 0x16,
        title: "Keyboard services (BIOS)",
        description: "Reading keys from the keyboard buffer. Type into the emulator screen to send keys to the program.",
        selector: "AH",
        functions: &[
            func!(
                0x00,
                "Wait for a key",
                "",
                "AH = scan code, AL = ASCII code (0 for special keys such as arrows and F1..F10). The key is removed from the buffer.",
                "mov ah, 0\nint 16h\ncmp al, 27      ; Esc?"
            ),
            func!(
                0x01,
                "Check for a key",
                "",
                "ZF = 1: no key; ZF = 0: AX = key (it stays in the buffer)",
                "mov ah, 1\nint 16h\njz no_key"
            ),
            func!(0x02, "Get shift flags", "", "AL = state of Shift, Ctrl, Alt, ... keys", "mov ah, 2\nint 16h"),
        ],
    },
    Interrupt {
        number: 0x21,
        title: "DOS services",
        description: "The operating system functions: console input/output, files, time, memory, program exit. File names are relative to the virtual drive C:: ~/Documents/RustEmu8086/vdrive/C on the Mac (or the folder of the program, see options), a drive in the browser in the web version. On errors file functions set CF = 1 and AX = error code.",
        selector: "AH",
        functions: &[
            func!(0x01, "Read a character with echo", "", "AL = character (waits for a key)", "mov ah, 1\nint 21h"),
            func!(0x02, "Write a character", "DL = character", "", "mov ah, 2\nmov dl, 'A'\nint 21h"),
            func!(0x05, "Write a character to the printer", "DL = character", "", "mov ah, 5\nmov dl, 'P'\nint 21h"),
            func!(
                0x06,
                "Direct console input/output",
                "DL = 0FFh: read a key without waiting; otherwise DL = character to write",
                "Input: ZF = 1 if no key, else AL = character",
                "mov ah, 6\nmov dl, 0FFh\nint 21h\njz no_key"
            ),
            func!(0x07, "Read a character without echo", "", "AL = character", "mov ah, 7\nint 21h"),
            func!(
                0x08,
                "Read a character without echo (Ctrl+Break checked)",
                "",
                "AL = character",
                "mov ah, 8\nint 21h"
            ),
            func!(
                0x09,
                "Write a string",
                "DS:DX = string ending with '$'",
                "",
                "mov ah, 9\nlea dx, msg\nint 21h\nmsg db 'Hello!$'"
            ),
            func!(
                0x0A,
                "Read a line (buffered input)",
                "DS:DX = buffer: byte 0 = maximum characters (including Enter), byte 1 = filled in with the count, then the characters and 0Dh",
                "",
                "mov ah, 0Ah\nlea dx, buffer\nint 21h\nbuffer db 20, ?, 20 dup(?)"
            ),
            func!(
                0x0B,
                "Check keyboard status",
                "",
                "AL = 0FFh if a key is waiting, 0 otherwise",
                "mov ah, 0Bh\nint 21h"
            ),
            func!(
                0x0C,
                "Flush the keyboard buffer, then call function AL",
                "AL = 01h, 06h, 07h, 08h or 0Ah",
                "As the called function",
                "mov ax, 0C01h\nint 21h"
            ),
            func!(0x19, "Get the current drive", "", "AL = drive (0 = A:, 2 = C:)", "mov ah, 19h\nint 21h"),
            func!(
                0x25,
                "Set an interrupt vector",
                "AL = interrupt number, DS:DX = new handler",
                "",
                "mov ax, 251Ch\nlea dx, my_timer\nint 21h"
            ),
            func!(
                0x2A,
                "Get the date",
                "",
                "CX = year, DH = month, DL = day, AL = day of the week (0 = Sunday)",
                "mov ah, 2Ah\nint 21h"
            ),
            func!(
                0x2C,
                "Get the time",
                "",
                "CH = hour, CL = minute, DH = second, DL = 1/100 seconds",
                "mov ah, 2Ch\nint 21h"
            ),
            func!(0x30, "Get the DOS version", "", "AL = major, AH = minor version", "mov ah, 30h\nint 21h"),
            func!(
                0x35,
                "Get an interrupt vector",
                "AL = interrupt number",
                "ES:BX = handler",
                "mov ax, 3510h\nint 21h"
            ),
            func!(
                0x39,
                "Create a directory",
                "DS:DX = path (zero-terminated)",
                "CF = 1 on error",
                "mov ah, 39h\nlea dx, dir\nint 21h\ndir db 'c:\\test', 0"
            ),
            func!(0x3A, "Remove a directory", "DS:DX = path", "CF = 1 on error", "mov ah, 3Ah\nlea dx, dir\nint 21h"),
            func!(
                0x3B,
                "Change the current directory",
                "DS:DX = path",
                "CF = 1 on error",
                "mov ah, 3Bh\nlea dx, dir\nint 21h"
            ),
            func!(
                0x3C,
                "Create a file",
                "DS:DX = file name, CX = attributes (0 = normal)",
                "AX = handle, CF = 1 on error",
                "mov ah, 3Ch\nmov cx, 0\nlea dx, fname\nint 21h\njc error\nmov handle, ax\nfname db 'test.txt', 0"
            ),
            func!(
                0x3D,
                "Open a file",
                "DS:DX = file name, AL = mode: 0 read, 1 write, 2 read/write",
                "AX = handle, CF = 1 on error",
                "mov ax, 3D00h\nlea dx, fname\nint 21h\njc error"
            ),
            func!(0x3E, "Close a file", "BX = handle", "CF = 1 on error", "mov ah, 3Eh\nmov bx, handle\nint 21h"),
            func!(
                0x3F,
                "Read from a file or the keyboard",
                "BX = handle (0 = keyboard), CX = byte count, DS:DX = buffer",
                "AX = bytes read (0 = end of file), CF = 1 on error",
                "mov ah, 3Fh\nmov bx, handle\nmov cx, 100\nlea dx, buffer\nint 21h"
            ),
            func!(
                0x40,
                "Write to a file or the screen",
                "BX = handle (1 = screen), CX = byte count, DS:DX = data",
                "AX = bytes written, CF = 1 on error",
                "mov ah, 40h\nmov bx, 1\nmov cx, 5\nlea dx, msg\nint 21h"
            ),
            func!(0x41, "Delete a file", "DS:DX = file name", "CF = 1 on error", "mov ah, 41h\nlea dx, fname\nint 21h"),
            func!(
                0x42,
                "Move the file pointer (seek)",
                "BX = handle, AL = origin (0 start, 1 current, 2 end), CX:DX = offset",
                "DX:AX = new position, CF = 1 on error",
                "mov ax, 4200h\nmov bx, handle\nxor cx, cx\nxor dx, dx\nint 21h"
            ),
            func!(
                0x43,
                "Get / set file attributes",
                "AL = 0 get, 1 set; DS:DX = file name; CX = attributes to set",
                "CX = attributes, CF = 1 if the file does not exist",
                "mov ax, 4300h\nlea dx, fname\nint 21h\njc not_found"
            ),
            func!(
                0x47,
                "Get the current directory",
                "DL = drive (0 = current), DS:SI = 64-byte buffer",
                "The path is stored without the drive letter",
                "mov ah, 47h\nmov dl, 0\nlea si, path\nint 21h"
            ),
            func!(0x4C, "Terminate the program", "AL = exit code", "", "mov ax, 4C00h\nint 21h"),
            func!(
                0x4E,
                "Find the first file",
                "DS:DX = mask (e.g. '*.txt'), CX = attributes",
                "The DTA is filled (name at offset 1Eh); CF = 1 if nothing found",
                "mov ah, 4Eh\nmov cx, 0\nlea dx, mask\nint 21h"
            ),
            func!(0x4F, "Find the next file", "", "CF = 1 when there are no more files", "mov ah, 4Fh\nint 21h"),
            func!(
                0x56,
                "Rename a file",
                "DS:DX = old name, ES:DI = new name",
                "CF = 1 on error",
                "mov ah, 56h\nlea dx, old\nlea di, new\nint 21h"
            ),
        ],
    },
    Interrupt {
        number: 0x20,
        title: "Terminate the program (DOS)",
        description: "Ends a COM program. The final RET of a COM program jumps to INT 20h at the start of the PSP.",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x1A,
        title: "Time services (BIOS)",
        description: "System timer and real-time clock.",
        selector: "AH",
        functions: &[
            func!(
                0x00,
                "Get the tick count",
                "",
                "CX:DX = timer ticks since midnight (about 18.2 per second)",
                "mov ah, 0\nint 1Ah\n; DX changes 18 times a second"
            ),
            func!(
                0x02,
                "Get the time (BCD)",
                "",
                "CH = hours, CL = minutes, DH = seconds (packed BCD)",
                "mov ah, 2\nint 1Ah"
            ),
            func!(0x04, "Get the date (BCD)", "", "CX = year, DH = month, DL = day (packed BCD)", "mov ah, 4\nint 1Ah"),
        ],
    },
    Interrupt {
        number: 0x15,
        title: "System services (BIOS)",
        description: "Miscellaneous BIOS functions.",
        selector: "AH",
        functions: &[func!(
            0x86,
            "Wait",
            "CX:DX = interval in microseconds",
            "CF = 0",
            "mov cx, 0Fh\nmov dx, 4240h   ; 1 000 000 us = 1 s\nmov ah, 86h\nint 15h"
        )],
    },
    Interrupt {
        number: 0x13,
        title: "Disk services (BIOS)",
        description: "Sector access to the virtual floppy (Emulator -> Virtual drive). 80 cylinders, 2 heads, 18 sectors of 512 bytes.",
        selector: "AH",
        functions: &[
            func!(0x00, "Reset the disk system", "DL = drive", "CF = 0", "mov ah, 0\nmov dl, 0\nint 13h"),
            func!(
                0x02,
                "Read sectors",
                "AL = count, CH = cylinder, CL = sector (1..18), DH = head, DL = drive (0 = A:), ES:BX = buffer",
                "AL = sectors read, CF = 1 on error",
                "mov ax, 0201h\nmov cx, 0002h\nmov dx, 0\nmov bx, 0800h\nmov es, bx\nxor bx, bx\nint 13h"
            ),
            func!(
                0x03,
                "Write sectors",
                "Same registers as AH = 02h",
                "AL = sectors written, CF = 1 on error",
                "mov ax, 0301h\nmov cx, 0001h\nmov dx, 0\nlea bx, sector\nint 13h"
            ),
            func!(
                0x08,
                "Get drive parameters",
                "DL = drive",
                "CH = last cylinder, CL = sectors per track, DH = last head",
                "mov ah, 8\nmov dl, 0\nint 13h"
            ),
        ],
    },
    Interrupt {
        number: 0x11,
        title: "Get the equipment list (BIOS)",
        description: "AX = equipment word (same as 0040:0010).",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x12,
        title: "Get the memory size (BIOS)",
        description: "AX = kilobytes of conventional memory (640).",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x19,
        title: "Reboot (BIOS)",
        description: "Restarts the computer; in the emulator the program ends.",
        selector: "",
        functions: &[],
    },
    Interrupt {
        number: 0x33,
        title: "Mouse services",
        description: "Mouse position and buttons in the emulator screen. Coordinates are on a 640x200 grid (in mode 13h divide X by 2).",
        selector: "AX",
        functions: &[
            func!(
                0x0000,
                "Reset the mouse",
                "",
                "AX = 0FFFFh if a mouse is installed, BX = number of buttons",
                "mov ax, 0\nint 33h"
            ),
            func!(0x0001, "Show the mouse cursor", "", "", "mov ax, 1\nint 33h"),
            func!(0x0002, "Hide the mouse cursor", "", "", "mov ax, 2\nint 33h"),
            func!(
                0x0003,
                "Get position and buttons",
                "",
                "CX = X, DX = Y, BX = buttons (bit 0 left, bit 1 right, bit 2 middle)",
                "mov ax, 3\nint 33h"
            ),
            func!(0x0004, "Set the position", "CX = X, DX = Y", "", "mov ax, 4\nmov cx, 320\nmov dx, 100\nint 33h"),
        ],
    },
];
