use vibefreak_core::VibeFreakEmulator;
use std::path::PathBuf;

fn get_fw_paths() -> (PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let base = PathBuf::from("original_firmware/unpacked");
    (
        base.join("nanowave_main_payload_stm32f446.bin"),
        base.join("userwavetable_factory_payload_qspi.bin"),
        base.join("nanowave_iochip_oled_payload_stm32f0.bin"),
        base.join("nanowave_iochip_matrix_payload_stm32f0.bin"),
        base.join("nanowave_iochip_kbd_payload_stm32f0.bin"),
    )
}

#[test]
fn test_swd_basic_commands() {
    let (main_bin, qspi_bin, oled_bin, matrix_bin, kbd_bin) = get_fw_paths();
    if !main_bin.exists() {
        eprintln!("Skipping test: firmware binaries not found");
        return;
    }

    let mut emu = VibeFreakEmulator::new(&main_bin, Some(&qspi_bin), &oled_bin, &matrix_bin, &kbd_bin)
        .expect("Failed to initialize emulator");

    // Test help command
    let help_out = emu.execute_swd_command("help");
    assert!(help_out.contains("SWD / JTAG Debugger Commands"));
    assert!(help_out.contains("regs"));

    // Test status command
    let status_out = emu.execute_swd_command("status");
    assert!(status_out.contains("STM32F446xE"));
    assert!(status_out.contains("RUNNING"));

    // Test halt & resume
    let halt_out = emu.execute_swd_command("halt");
    assert!(halt_out.contains("Target halted"));
    assert!(emu.is_halted());

    let resume_out = emu.execute_swd_command("resume");
    assert!(resume_out.contains("Target resumed"));
    assert!(!emu.is_halted());

    // Test registers
    let regs_out = emu.execute_swd_command("regs");
    assert!(regs_out.contains("R0:"));
    assert!(regs_out.contains("PC:"));
    assert!(regs_out.contains("SP:"));

    // Test register write & read
    let write_reg = emu.execute_swd_command("reg r0 0x1234");
    assert!(write_reg.contains("Set R0 = 0x00001234"));
    let read_reg = emu.execute_swd_command("reg r0");
    assert!(read_reg.contains("R0 = 0x00001234"));

    // Test memory read & write (SRAM at 0x20000000)
    let mww_out = emu.execute_swd_command("mww 0x20000000 0xDEADBEEF");
    assert!(mww_out.contains("Wrote 0xDEADBEEF to 0x20000000"));

    let mdw_out = emu.execute_swd_command("mdw 0x20000000 1");
    assert!(mdw_out.contains("0xDEADBEEF"));

    // Test hexdump
    let dump_out = emu.execute_swd_command("dump 0x20000000 16");
    assert!(dump_out.contains("0x20000000:"));

    // Test breakpoints
    let bp_set = emu.execute_swd_command("break 0x08025008");
    assert!(bp_set.contains("Breakpoint set at 0x08025008"));
    assert!(emu.swd.check_breakpoint(0x08025008));

    let bp_list = emu.execute_swd_command("breakpoints");
    assert!(bp_list.contains("0x08025008"));

    let bp_del = emu.execute_swd_command("del 0x08025008");
    assert!(bp_del.contains("removed"));
    assert!(!emu.swd.check_breakpoint(0x08025008));

    // Test symbols
    let sym_out = emu.execute_swd_command("sym");
    assert!(sym_out.contains("p_current_preset"));
    assert!(sym_out.contains("p_tempo_bpm"));

    // Test memmap
    let memmap_out = emu.execute_swd_command("memmap");
    assert!(memmap_out.contains("FLASH"));
    assert!(memmap_out.contains("QSPI"));
}

