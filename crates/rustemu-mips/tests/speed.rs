//! Speed of the MIPS machine, in millions of instructions a second, with and without the
//! step-back history (a loop of arithmetic and a loop of stores and loads). Not a check: run it
//! in release to measure, `cargo test -p rustemu-mips --release --test speed -- --ignored --nocapture`.

use rustemu_mips::Machine;

fn measure(src: &str, history: bool, budget: u64) -> f64 {
    let mut m = Machine::new();
    m.load(&rustemu_mips::asm::assemble(src).unwrap().image).unwrap();
    m.history_enabled = history;
    let t = std::time::Instant::now();
    let (_, n) = m.run_counted(budget);
    n as f64 / t.elapsed().as_secs_f64() / 1e6
}

#[test]
#[ignore = "a measurement: run in release"]
fn instructions_per_second() {
    let arithmetic = "main: li $t0, 50000000\nloop: addiu $t0, $t0, -1\naddu $t1, $t1, $t0\nbnez $t0, loop\n";
    let memory = "main: li $t0, 20000000\nloop: sw $t0, -4($sp)\nlw $t1, -4($sp)\naddiu $t0, $t0, -1\nbnez $t0, loop\n";
    for (name, src) in [("arithmetic", arithmetic), ("loads and stores", memory)] {
        let plain = measure(src, false, u64::MAX);
        let history = measure(src, true, 30_000_000);
        println!("{name}: {plain:.1} M instructions/s, {history:.1} M with the step-back history");
    }
}
