use rustemu_asm::{Dialect, FileLoader, assemble_dialect};

struct NoIncludes;
impl FileLoader for NoIncludes {
    fn load(&mut self, _: &str, _: &str) -> Option<(String, String)> {
        None
    }
}
fn a86(src: &str) -> rustemu_asm::Program {
    assemble_dialect("test.a86", src, &mut NoIncludes, Some(Dialect::A86)).unwrap()
}

#[test]
fn numeric_bases_are_distinct_from_masm_and_nasm() {
    let p = a86("dw 0100,0110b,0110xb,0100d,0100xd,2k\nint 020\n");
    assert_eq!(p.image, [0, 1, 0x0b, 0x11, 6, 0, 0x0d, 0x10, 100, 0, 0, 8, 0xcd, 0x20]);
    assert_eq!(p.dialect, Dialect::A86);
    for d in [Dialect::Masm, Dialect::Nasm] {
        let p = assemble_dialect("test.asm", "dw 0100\nint 020\n", &mut NoIncludes, Some(d)).unwrap();
        assert_eq!(p.image, [100, 0, 0xcd, 20]);
    }
}

#[test]
fn detection_does_not_capture_unrelated_emu_directives_or_comments() {
    use rustemu_asm::detect_dialect;
    assert_eq!(detect_dialect("a.asm", "#emu8086#\norg 100h\nret\n"), Dialect::Masm);
    assert_eq!(detect_dialect("a.asm", "; #em\nbits 16\n"), Dialect::Nasm);
    assert_eq!(detect_dialect("a.asm", "clear macro\nxor #1,#1\n#em ; done\n"), Dialect::A86);
}

#[test]
fn byte_combination_bit_and_typed_memory() {
    let p = a86("mov ax,'A' by 10\nmov b[si],bit 3\nmov w[bx],0100\nint 020\n");
    assert_eq!(p.image, [0xb8, 10, 65, 0xc6, 4, 8, 0xc7, 7, 0, 1, 0xcd, 0x20]);
}

#[test]
fn dup_empty_declarations_and_redefinable_equ() {
    let p = a86("sz equ 3\nx db sz dup 1\nsz equ $-x\ny dw\ndb sz\n");
    assert_eq!(p.image, [1, 1, 1, 3]);
    assert_eq!(p.symbols.iter().find(|s| s.name == "Y").unwrap().size, Some(2));
}

#[test]
fn repeated_local_labels_have_directional_references() {
    let p = a86("l1: nop\njmp >l1\nint 3\nl1: nop\njmp l1\nint 020\n");
    assert_eq!(p.image, [0x90, 0xeb, 1, 0xcc, 0x90, 0xeb, 0xfd, 0xcd, 0x20]);
}

#[test]
fn a86_macro_parameters_repeats_and_debug_locations() {
    let p = a86("clear macro\n#rx1l\nxor #x,#x ; a comment with # does not expand\n#em\nclear ax,bx\nint 020\n");
    assert_eq!(p.image, [0x31, 0xc0, 0x31, 0xdb, 0xcd, 0x20]);
    assert!(p.lines.iter().take(2).all(|l| l.line == 5 && l.origin.is_some()));
}

#[test]
fn structures_are_typed_overlays_without_allocating_bytes() {
    let p = a86("struc\nlo db ?\nhi dw ?\npacket ends\nmov lo,1\nmov hi,0100\ndw packet\n");
    assert_eq!(p.image, [0xc6, 6, 0, 0, 1, 0xc7, 6, 1, 0, 0, 1, 3, 0]);
}

#[test]
fn macro_include_and_original_file_table() {
    struct Includes;
    impl FileLoader for Includes {
        fn load(&mut self, n: &str, _: &str) -> Option<(String, String)> {
            (n == "exit.inc").then(|| (n.into(), "quit macro\nmov ah,04c\nint 021\n#em\n".into()))
        }
    }
    let p = assemble_dialect("test.8", "include exit.inc\nquit\n", &mut Includes, None).unwrap();
    assert_eq!(p.image, [0xb4, 0x4c, 0xcd, 0x21]);
    assert_eq!(p.files, ["test.8", "exit.inc"]);
}

#[test]
fn shorthand_preserves_registers_and_order() {
    let p = a86("mov ax,bx,1\npush ax,bx\npop bx,ax\nstosw 3\n");
    assert_eq!(p.image, [0xbb, 1, 0, 0x89, 0xd8, 0x50, 0x53, 0x5b, 0x58, 0xab, 0xab, 0xab]);
}

#[test]
fn a386_is_reported_explicitly_without_dialect_fallback() {
    let e = assemble_dialect("test.8x", "mov eax,1\n", &mut NoIncludes, None).unwrap_err();
    assert!(e.to_string().contains("requires a 386 target"));
}

#[test]
fn segment_loads_enter_and_bound() {
    let p = a86("mov es,0b800\nmov ds,es\nenter\nbound ax,w[bx]\n");
    assert_eq!(p.image, [0x50, 0xb8, 0, 0xb8, 0x8e, 0xc0, 0x58, 0x06, 0x1f, 0x55, 0x89, 0xe5, 0x62, 0x07]);
}

#[test]
fn floating_expressions_and_stack_shorthand() {
    let p = a86("dd 1.0+2.0*3, 2 dup (2.0/4)\nfld 0\nfadd 2\nfstp 3\n");
    assert_eq!(&p.image[..12], &[0, 0, 0xe0, 0x40, 0, 0, 0, 0x3f, 0, 0, 0, 0x3f]);
    assert_eq!(&p.image[12..], &[0xd9, 0xc0, 0xd8, 0xc2, 0xdd, 0xdb]);
}

#[test]
fn numeric_loops_can_export_labels_and_constants_survive_macro_calls() {
    let p = a86("p1 equ 2\nmake macro\n#rx1(3)\nm#nx: db #1\n#em\nmake p1\ndw m2\n");
    assert_eq!(p.image, [2, 2, 2, 1, 1]);
}

#[test]
fn macro_loops_are_bounded_and_cancellable() {
    let src = "bomb macro\n#rx1(65536)\n#ry1(65536)\n#er\n#er\n#em\nbomb\n";
    let e = assemble_dialect("test.a86", src, &mut NoIncludes, Some(Dialect::A86)).unwrap_err();
    assert!(e.to_string().contains("budget"), "{e}");
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let e =
        rustemu_asm::assemble_cancellable("test.a86", src, &mut NoIncludes, Some(Dialect::A86), cancel).unwrap_err();
    assert!(e.to_string().contains("cancelled"), "{e}");
    // A countdown loop is bounded by its first (start) value too, before anything is allocated.
    let src = "bomb macro\n#qx(4000000000)1\nnop\n#er\n#em\nbomb\n";
    let e = assemble_dialect("test.a86", src, &mut NoIncludes, Some(Dialect::A86)).unwrap_err();
    assert!(e.to_string().contains("loop count exceeds resource budget"), "{e}");
    assert_eq!(a86("down macro\n#qx31\nnop\n#er\n#em\ndown\n").image, [0x90; 3]);
}

#[test]
fn a86_typed_offset_arithmetic() {
    let p = a86("x db 1\ny dw 2\ndb type (x-y),type (y-x),type (1-y),type (y+x)\ndw offset y-x\n");
    assert_eq!(p.image, [1, 2, 0, 1, 2, 0, 2, 1, 0]);
}

#[test]
fn labelled_segment_override_and_repeat_count() {
    let p = a86("l1: cs mov [bx],al\ninc ax,3\ninc b -4[si](2*bit 1)\n");
    assert_eq!(p.image, [0x2e, 0x88, 0x07, 0x40, 0x40, 0x40, 0xfe, 0x04]);
}

#[test]
fn large_structure_offsets_do_not_build_deep_expression_trees() {
    let mut src = String::from("struc\n");
    for i in 0..1000 {
        src.push_str(&format!("field_{i} db ?\n"));
    }
    src.push_str("record ends\ndw record\n");
    assert_eq!(a86(&src).image, [0xe8, 3]);
}

#[test]
fn nested_macro_calls_keep_local_forward_data_separate() {
    let p =
        a86("load macro\nmov al,m1\njmp >m2\nm1 db #1\nm2:\n#em\ntwice macro\nload 1\nload 2\n#em\ntwice\nint 020\n");
    let data: Vec<_> = p.symbols.iter().filter(|s| s.display_name() == "M1").collect();
    assert_eq!(data.len(), 2);
    let loads: Vec<_> = p.listing.iter().filter(|l| l.bytes.first() == Some(&0xa0)).collect();
    assert_eq!(loads.len(), 2);
    let addresses: Vec<_> = loads.iter().map(|l| u16::from_le_bytes([l.bytes[1], l.bytes[2]])).collect();
    assert_ne!(addresses[0], addresses[1]);
}
