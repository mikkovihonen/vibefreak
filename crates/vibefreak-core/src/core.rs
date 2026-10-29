use std::path::Path;
use unicorn_engine::unicorn_const::{Arch, HookType, Mode};
use unicorn_engine::{Prot, RegisterARM, Unicorn};

use crate::config::*;
use crate::coproc::CoProcessorBusRouter;
use crate::nvic::NvicController;
use crate::peripherals::tlv320aic3104::{AudioInputSource, HeadsetJackState};
use crate::peripherals::MmioBus;

pub struct EmulatorContext {
    pub mmio: MmioBus,
    pub nvic: NvicController,
    pub tx_bytes: Vec<u8>,
    pub usart1_tx_bytes: Vec<u8>,
    pub boot_splash_finished: bool,
    pub recent_pcs: std::collections::VecDeque<u32>,
    pub mmio_read_count: u64,
    pub mmio_write_count: u64,
}

pub struct VibeFreakEmulator {
    pub uc: Unicorn<'static, EmulatorContext>,
    pub coproc: CoProcessorBusRouter,
    pub profile: crate::profile::FirmwareProfile,
    pub pc: u32,
    pub is_booted: bool,
    pub usart1_isr: u32,
    pub usart2_isr: u32,
    pub usart3_isr: u32,
    pub otg_fs_isr: u32,
    pub dma2_stream4_isr: u32,
    pub dma2_stream6_isr: u32,
    pub dma2_stream0_isr: u32,
    pub exti15_10_isr: u32,
    pub dma_ping: bool,
    pub audio_driver_addr: u32,
    pub systick_cycle_accum: u32,
    pub audio_proc: crate::audio::AudioProcessor,
    pub profiling: bool,
    pub perf_step_mcu: std::time::Duration,
    pub perf_coproc: std::time::Duration,
    pub perf_render_audio: std::time::Duration,
    pub perf_usart: std::time::Duration,
    pub perf_fast_forward: std::time::Duration,
    pub perf_systick: std::time::Duration,
    pub perf_render_enter_exc: std::time::Duration,
    pub perf_render_emu_start: std::time::Duration,
    pub perf_render_exit_exc: std::time::Duration,
    pub perf_render_dsp: std::time::Duration,
    pub block_counter: usize,
    pub initial_pot_idx: usize,
    pub activity_countdown: usize,
    pub audio_input_countdown: usize,
    pub swd: crate::swd::SwdDebugger,
    coproc_req_tx: crossbeam_channel::Sender<usize>,
    coproc_resp_rx: crossbeam_channel::Receiver<std::time::Duration>,
    coproc_worker_handle: Option<std::thread::JoinHandle<()>>,
}

/// SAFETY: VibeFreakEmulator owns its Unicorn instance exclusively. Unicorn uses an internal
/// thread-local C context during emu_start/hooks, meaning a single VibeFreakEmulator instance
/// must not be accessed concurrently from multiple threads. Moving the emulator across threads when
/// not actively executing is safe because all state is uniquely owned.
unsafe impl Send for VibeFreakEmulator {}

impl Drop for VibeFreakEmulator {
    fn drop(&mut self) {
        let _ = self.coproc_req_tx.send(0);
        if let Some(h) = self.coproc_worker_handle.take() {
            let _ = h.join();
        }
    }
}

#[inline(always)]
fn is_destructive_fifo_reg(addr: u32) -> bool {
    matches!(
        addr,
        0x4001_1004 | 0x4000_4404 | 0x4000_4804 // USART1_DR, USART2_DR, USART3_DR
        | 0x4001_300C | 0x4000_380C | 0x4000_3C0C // SPI1_DR, SPI2_DR, SPI3_DR
        | 0x4000_5410 | 0x4000_5810 | 0x4000_5C10 // I2C1_DR, I2C2_DR, I2C3_DR
        | 0xA000_1020 // QUADSPI_DR
    )
}

fn apply_qspi_data(flash: &mut [u8], data: &[u8]) {
    if data.len() == QSPI_FLASH_SIZE {
        flash.copy_from_slice(data);
        if flash[QSPI_STAGING_SECTOR_BASE..QSPI_STAGING_SECTOR_BASE + 4] == [0xFF, 0xFF, 0xFF, 0xFF] {
            for slot in 1..=128u16 {
                let patch_idx = ((slot - 1) as usize) % crate::factory_data::FACTORY_PATCHES.len();
                let patch = &crate::factory_data::FACTORY_PATCHES[patch_idx];
                let p_data = crate::factory_data::build_factory_preset(slot, patch.name, Some(patch));
                let active_addr = QSPI_STAGING_SECTOR_BASE + ((slot - 1) as usize) * 0x1000;
                if active_addr + 4096 <= QSPI_SAMPLE_MEMORY_BASE {
                    flash[active_addr..active_addr + 4096].copy_from_slice(&p_data);
                }
            }

            // Dedicated Factory Vocoder Presets (slots 208..=223)
            let vocoder_patch = crate::factory_data::FactoryPatchDef {
                name: "Vocoder Classic",
                engine_type: 14,
                cutoff: 0x7FFF,
                resonance: 0,
                filter_mode: 0,
                attack: 10,
                decay: 1500,
                sustain: 0x7FFF,
            };
            for slot in 208..=223u16 {
                let p_data = crate::factory_data::build_factory_preset(slot, vocoder_patch.name, Some(&vocoder_patch));
                let active_addr = QSPI_STAGING_SECTOR_BASE + ((slot - 1) as usize) * 0x1000;
                if active_addr + 4096 <= QSPI_SAMPLE_MEMORY_BASE {
                    flash[active_addr..active_addr + 4096].copy_from_slice(&p_data);
                }
            }
        }

        let has_valid_samples = if flash[QSPI_SAMPLE_MEMORY_BASE..QSPI_SAMPLE_MEMORY_BASE + 4] == [0xEF, 0xAA, 0x55, 0xFA] {
            let sample_len = u32::from_le_bytes(
                flash[QSPI_SAMPLE_MEMORY_BASE + 4..QSPI_SAMPLE_MEMORY_BASE + 8]
                    .try_into()
                    .unwrap(),
            );
            let token_id = u32::from_le_bytes(
                flash[QSPI_SAMPLE_MEMORY_BASE + 8..QSPI_SAMPLE_MEMORY_BASE + 12]
                    .try_into()
                    .unwrap(),
            );
            token_id > 0 && sample_len >= 40_000
        } else {
            false
        };

        if !has_valid_samples {
            let mut sample_addr = QSPI_SAMPLE_MEMORY_BASE;
            for sample_id in 0..64u8 {
                let pcm = crate::factory_data::generate_sample_waveform(sample_id);
                let sample_sector = crate::factory_data::build_factory_sample_sector(sample_id, &pcm);
                if sample_addr + sample_sector.len() <= QSPI_SAMPLE_MEMORY_END {
                    flash[sample_addr..sample_addr + sample_sector.len()].copy_from_slice(&sample_sector);
                    sample_addr += sample_sector.len();
                } else {
                    break;
                }
            }
        }

        if flash[QSPI_STOCK_WAVETABLE_DEST_ADDR..QSPI_STOCK_WAVETABLE_DEST_ADDR + 4] == [0xFF, 0xFF, 0xFF, 0xFF] {
            if crate::factory_data::FACTORY_WAVETABLES.len() >= 0x0005_0000 {
                flash[QSPI_STOCK_WAVETABLE_DEST_ADDR..QSPI_STOCK_WAVETABLE_DEST_ADDR + QSPI_STOCK_WAVETABLE_SIZE_BYTES]
                    .copy_from_slice(&crate::factory_data::FACTORY_WAVETABLES[0x0001_0000..0x0001_0000 + QSPI_STOCK_WAVETABLE_SIZE_BYTES]);
            }
        }
        if flash[QSPI_WAVETABLE_DEST_ADDR..QSPI_WAVETABLE_DEST_ADDR + 4] == [0xFF, 0xFF, 0xFF, 0xFF] {
            let wt_len = crate::factory_data::FACTORY_WAVETABLES.len().min(QSPI_WAVETABLE_SIZE_BYTES);
            flash[QSPI_WAVETABLE_DEST_ADDR..QSPI_WAVETABLE_DEST_ADDR + wt_len]
                .copy_from_slice(&crate::factory_data::FACTORY_WAVETABLES[..wt_len]);
        }
        if flash[QSPI_STATUS_SECTOR_BASE] == 0xFF {
            flash[QSPI_STATUS_SECTOR_BASE] = 0x51;
        }
    } else if data.len() == QSPI_WAVETABLE_SIZE_BYTES {
        flash[QSPI_WAVETABLE_DEST_ADDR..QSPI_WAVETABLE_DEST_ADDR + data.len()].copy_from_slice(data);
        if data.len() >= 0x0005_0000 {
            flash[QSPI_STOCK_WAVETABLE_DEST_ADDR..QSPI_STOCK_WAVETABLE_DEST_ADDR + QSPI_STOCK_WAVETABLE_SIZE_BYTES]
                .copy_from_slice(&data[0x0001_0000..0x0001_0000 + QSPI_STOCK_WAVETABLE_SIZE_BYTES]);
        }
    } else if data.len() >= 64 && (data[0] & 0x80) != 0 {
        let qspi_dest = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let qspi_size = u32::from_le_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let payload_len = (data.len() - 64).min(qspi_size);
        if qspi_dest + payload_len <= flash.len() {
            flash[qspi_dest..qspi_dest + payload_len].copy_from_slice(&data[64..64 + payload_len]);
        }
        if qspi_dest == QSPI_WAVETABLE_DEST_ADDR && payload_len >= 0x0005_0000 {
            flash[QSPI_STOCK_WAVETABLE_DEST_ADDR..QSPI_STOCK_WAVETABLE_DEST_ADDR + QSPI_STOCK_WAVETABLE_SIZE_BYTES]
                .copy_from_slice(&data[64 + 0x0001_0000..64 + 0x0001_0000 + QSPI_STOCK_WAVETABLE_SIZE_BYTES]);
        }
    } else if data.len() >= 4 && data[0..2] == [0x01, 0x19] && data.len() <= 4096 {
        flash[QSPI_STAGING_SECTOR_BASE..QSPI_STAGING_SECTOR_BASE + data.len()].copy_from_slice(data);
    } else {
        let write_len = data.len().min(flash.len());
        flash[..write_len].copy_from_slice(&data[..write_len]);
    }
}

impl VibeFreakEmulator {
    /// Boot emulator directly from an Arturia .mff firmware package file.
    pub fn from_mff_file<P: AsRef<Path>>(mff_path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let pkg = crate::mff::MffFirmwarePackage::from_file(mff_path)?;
        Self::from_package(&pkg)
    }

    /// Boot emulator directly from raw bytes of an Arturia .mff package archive.
    pub fn from_mff_bytes(mff_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
        let pkg = crate::mff::MffFirmwarePackage::from_bytes(mff_bytes)?;
        Self::from_package(&pkg)
    }

    /// Boot emulator from an already parsed `MffFirmwarePackage`.
    pub fn from_package(pkg: &crate::mff::MffFirmwarePackage) -> Result<Self, Box<dyn std::error::Error>> {
        Self::from_buffers(
            &pkg.main_mcu,
            Some(&pkg.qspi_wavetable),
            &pkg.oled_coproc,
            &pkg.matrix_coproc,
            &pkg.kbd_coproc,
        )
    }

    /// Boot emulator from raw memory buffers without accessing the filesystem.
    pub fn from_buffers(
        main_bin: &[u8],
        qspi_bin: Option<&[u8]>,
        oled_bin: &[u8],
        matrix_bin: &[u8],
        kbd_bin: &[u8],
    ) -> Result<Self, Box<dyn std::error::Error>> {
        if main_bin.len() < 8 {
            return Err("Main firmware payload too small".into());
        }

        let profile = crate::profile::FirmwareProfile::detect(main_bin);

        let nvic = NvicController::new(profile.vtor);

        let ctx = EmulatorContext {
            mmio: MmioBus::new(),
            nvic,
            tx_bytes: Vec::new(),
            usart1_tx_bytes: Vec::new(),
            boot_splash_finished: false,
            recent_pcs: std::collections::VecDeque::new(),
            mmio_read_count: 0,
            mmio_write_count: 0,
        };

        let mut uc = Unicorn::new_with_data(Arch::ARM, Mode::THUMB | Mode::MCLASS, ctx)?;

        // Map Flash: 0x08000000 (1 MB) and pre-fill with erased NOR Flash state (0xFF)
        uc.mem_map(FLASH_BASE, FLASH_SIZE as u64, Prot::ALL)?;
        let ff_chunk = [0xFFu8; 64 * 1024];
        for off in (0..FLASH_SIZE).step_by(ff_chunk.len()) {
            let chunk = ff_chunk.len().min(FLASH_SIZE - off);
            let _ = uc.mem_write(FLASH_BASE + off as u64, &ff_chunk[..chunk]);
        }
        // Write firmware payload at profile.load_addr
        uc.mem_write(profile.load_addr as u64, &main_bin)?;

        // Map Architectural EXC_RETURN Trap Page: 0xFFFFF000 (4 KB)
        uc.mem_map(EXC_RETURN_PAGE_BASE, EXC_RETURN_PAGE_SIZE as u64, Prot::ALL)?;
        // Fill entire trap page with Thumb `b .` (\xfe\xe7)
        let trap_bytes = [0xFE, 0xE7].repeat(EXC_RETURN_PAGE_SIZE / 2);
        uc.mem_write(EXC_RETURN_PAGE_BASE, &trap_bytes)?;

        // Code hook on EXC_RETURN page to immediately stop emulation when returning from ISR
        uc.add_code_hook(
            EXC_RETURN_PAGE_BASE,
            EXC_RETURN_PAGE_BASE + EXC_RETURN_PAGE_SIZE as u64 - 1,
            |uc, _addr, _size| {
                let _ = uc.emu_stop();
            },
        )?;

        // Map SRAM: 0x20000000 (128 KB)
        uc.mem_map(SRAM_BASE, SRAM_SIZE as u64, Prot::ALL)?;

        // Map QSPI Flash: 0x90000000 (16 MB) and initialize with factory stock data
        uc.mem_map(QSPI_FLASH_BASE, QSPI_FLASH_SIZE as u64, Prot::ALL)?;
        let mut qspi_flash_image = crate::factory_data::generate_factory_qspi_image();
        if let Some(qspi_data) = qspi_bin {
            apply_qspi_data(&mut qspi_flash_image, qspi_data);
        }
        let _ = uc.mem_write(QSPI_FLASH_BASE, &qspi_flash_image);
        uc.get_data_mut().mmio.qspi.load_flash_image(&qspi_flash_image);

        // Map QUADSPI MMIO Registers: 0xA0000000 (64 KB)
        uc.mem_map(QUADSPI_REG_BASE, QUADSPI_REG_SIZE as u64, Prot::ALL)?;

        // Map Peripheral MMIO: 0x40000000 (1 MB)
        uc.mem_map(PERIPH_MMIO_BASE, PERIPH_MMIO_SIZE as u64, Prot::ALL)?;

        // Map USB OTG FS MMIO: 0x50000000 (64 KB)
        uc.mem_map(USB_OTG_FS_BASE, USB_OTG_FS_SIZE as u64, Prot::ALL)?;

        // Map Boot Alias: 0x00000000 (1 MB) pre-filled with 0xFF
        uc.mem_map(BOOT_ALIAS_BASE, BOOT_ALIAS_SIZE as u64, Prot::ALL)?;
        for off in (0..BOOT_ALIAS_SIZE).step_by(ff_chunk.len()) {
            let chunk = ff_chunk.len().min(BOOT_ALIAS_SIZE - off);
            let _ = uc.mem_write(BOOT_ALIAS_BASE + off as u64, &ff_chunk[..chunk]);
        }
        // Write vector table at BOOT_ALIAS_BASE (0x0) for initial reset vector fetch
        uc.mem_write(
            BOOT_ALIAS_BASE,
            &main_bin[..main_bin.len().min(BOOT_ALIAS_SIZE)],
        )?;
        // Also mirror main_bin at the physical Flash offset within Boot Alias
        let alias_flash_offset = (profile.load_addr as u64).saturating_sub(FLASH_BASE);
        if (alias_flash_offset as usize) + main_bin.len() <= BOOT_ALIAS_SIZE {
            let _ = uc.mem_write(BOOT_ALIAS_BASE + alias_flash_offset, &main_bin);
        }

        // Map System Information Block: 0x1FFF0000 (64 KB)
        uc.mem_map(SYSTEM_MEM_BASE, SYSTEM_MEM_SIZE as u64, Prot::ALL)?;

        // Map PPB: 0xE0000000 (1 MB)
        uc.mem_map(PPB_BASE, PPB_SIZE as u64, Prot::ALL)?;

        // Map SRAM Bit-band Alias: 0x22000000 (32 MB)
        uc.mem_map(SRAM_BITBAND_BASE, SRAM_BITBAND_SIZE as u64, Prot::ALL)?;

        // Map Peripheral Bit-band Alias: 0x42000000 (32 MB)
        uc.mem_map(PERIPH_BITBAND_BASE, PERIPH_BITBAND_SIZE as u64, Prot::ALL)?;

        // Pre-populate hardware registers
        uc.mem_write(QUADSPI_CR as u64, &(0u32).to_le_bytes())?;
        uc.mem_write(RCC_CR as u64, &((1u32 << 0) | (1u32 << 1)).to_le_bytes())?;
        uc.mem_write(VTOR_ADDR as u64, &profile.vtor.to_le_bytes())?;
        uc.mem_write(CPACR_ADDR as u64, &(0xFu32 << 20).to_le_bytes())?;
        let gpioc_idr = uc.get_data().mmio.gpio.ports[2].idr;
        uc.mem_write(0x4002_0810, &gpioc_idr.to_le_bytes())?;
        let gpiod_idr = uc.get_data().mmio.gpio.ports[3].idr;
        uc.mem_write(0x4002_0C10, &gpiod_idr.to_le_bytes())?;

        // Set initial Stack Pointer, MSP, FPU, and Reset Handler
        let sp = u32::from_le_bytes(main_bin[0..4].try_into().unwrap());
        let reset_handler = u32::from_le_bytes(main_bin[4..8].try_into().unwrap());

        uc.reg_write(RegisterARM::SP, sp as u64)?;
        uc.reg_write(RegisterARM::MSP, sp as u64)?;
        uc.reg_write(RegisterARM::PC, (reset_handler | 1) as u64)?;
        let _ = uc.reg_write(RegisterARM::FPEXC, 1 << 30); // Enable VFP/FPU hardware coprocessor

        // Hook MMIO reads on APB/AHB (with unaligned byte/halfword access alignment)
        uc.add_mem_hook(
            HookType::MEM_READ,
            0x4000_0000,
            0x4010_0000 - 1,
            |uc, _access, addr, size, _val| {
                uc.get_data_mut().mmio_read_count += 1;
                let aligned_addr = (addr as u32) & !3;
                let shift = ((addr as u32) & 3) * 8;
                let reg_val = uc.get_data_mut().mmio.read(aligned_addr, 4);
                let val = (reg_val >> shift) as u32;
                let _ = uc.mem_write(addr, &val.to_le_bytes()[..size.min(4)]);
                true
            },
        )?;

        uc.add_mem_hook(
            HookType::MEM_READ,
            0xA000_0000,
            0xA001_0000 - 1,
            |uc, _access, addr, size, _val| {
                uc.get_data_mut().mmio_read_count += 1;
                let aligned_addr = (addr as u32) & !3;
                let shift = ((addr as u32) & 3) * 8;
                let reg_val = uc.get_data_mut().mmio.read(aligned_addr, size);
                let val = (reg_val >> shift) as u32;
                let _ = uc.mem_write(addr, &val.to_le_bytes()[..size.min(4)]);
                true
            },
        )?;

        // Hook MMIO reads on USB OTG FS (0x5000_0000..=0x5000_FFFF)
        uc.add_mem_hook(
            HookType::MEM_READ,
            USB_OTG_FS_BASE,
            USB_OTG_FS_BASE + USB_OTG_FS_SIZE as u64 - 1,
            |uc, _access, addr, size, _val| {
                uc.get_data_mut().mmio_read_count += 1;
                let aligned_addr = (addr as u32) & !3;
                let shift = ((addr as u32) & 3) * 8;
                let reg_val = uc.get_data_mut().mmio.read(aligned_addr, size);
                let val = (reg_val >> shift) as u32;
                let _ = uc.mem_write(addr, &val.to_le_bytes()[..size.min(4)]);
                true
            },
        )?;

        // Hook MMIO writes on APB/AHB (with unaligned byte/halfword access alignment)
        uc.add_mem_hook(
            HookType::MEM_WRITE,
            0x4000_0000,
            0x4010_0000 - 1,
            |uc, _access, addr, size, val| {
                uc.get_data_mut().mmio_write_count += 1;
                let addr_u32 = addr as u32;
                let aligned_addr = addr_u32 & !3;
                let shift = (addr_u32 & 3) * 8;
                let mask = match size {
                    1 => 0xFFu32,
                    2 => 0xFFFFu32,
                    _ => 0xFFFF_FFFFu32,
                };
                let raw_val = (val as u32) & mask;
                let full_val = if size < 4 {
                    if is_destructive_fifo_reg(aligned_addr) {
                        raw_val << shift
                    } else {
                        let cur = uc.get_data_mut().mmio.read(aligned_addr, 4);
                        (cur & !(mask << shift)) | (raw_val << shift)
                    }
                } else {
                    raw_val
                };

                uc.get_data_mut().mmio.write(aligned_addr, 4, full_val);

                if aligned_addr == (DMA2_BASE + 0x10 + 7 * 0x18) {
                    if Self::service_dma_stream(uc, 7) {
                        let _ = uc.emu_stop();
                    }
                } else if aligned_addr == USART1_DR {
                    let b = (raw_val & 0xFF) as u8;
                    uc.get_data_mut().usart1_tx_bytes.push(b);
                } else if aligned_addr == USART2_DR {
                    let b = (raw_val & 0xFF) as u8;
                    uc.get_data_mut().tx_bytes.push(b);
                } else if aligned_addr == DMA2_BASE + 0x08 {
                    let lisr = uc.get_data().mmio.dma2.lisr;
                    let _ = uc.mem_write(DMA2_BASE as u64, &lisr.to_le_bytes());
                } else if aligned_addr == DMA2_BASE + 0x0C {
                    let hisr = uc.get_data().mmio.dma2.hisr;
                    let _ = uc.mem_write(DMA2_BASE as u64 + 4, &hisr.to_le_bytes());
                } else if aligned_addr == DMA1_BASE + 0x08 {
                    let lisr = uc.get_data().mmio.dma1.lisr;
                    let _ = uc.mem_write(DMA1_BASE as u64, &lisr.to_le_bytes());
                } else if aligned_addr == DMA1_BASE + 0x0C {
                    let hisr = uc.get_data().mmio.dma1.hisr;
                    let _ = uc.mem_write(DMA1_BASE as u64 + 4, &hisr.to_le_bytes());
                } else if aligned_addr == 0x4002_3C0C {
                    let sr = uc.get_data().mmio.flash.sr;
                    let _ = uc.mem_write(0x4002_3C0C, &sr.to_le_bytes());
                }
                true
            },
        )?;

        // Hook System Control Space (0xE000_E000..=0xE000_EFFF)
        uc.add_mem_hook(
            HookType::MEM_READ,
            0xE000_E000,
            0xE000_EFFF,
            |uc, _access, addr, size, _val| {
                let val = uc.get_data_mut().nvic.read_ppb(addr as u32, size);
                let _ = uc.mem_write(addr, &val.to_le_bytes()[..size.min(4)]);
                true
            },
        )?;

        uc.add_mem_hook(
            HookType::MEM_WRITE,
            0xE000_E000,
            0xE000_EFFF,
            |uc, _access, addr, size, val| {
                let action = uc.get_data_mut().nvic.write_ppb(addr as u32, size, val as u32);
                if action == crate::nvic::PpbAction::SysResetReq {
                    let _ = uc.emu_stop();
                }
                true
            },
        )?;

        uc.add_mem_hook(
            HookType::MEM_WRITE,
            0xA000_0000,
            0xA001_0000 - 1,
            |uc, _access, addr, size, val| {
                let addr_u32 = addr as u32;
                let aligned_addr = addr_u32 & !3;
                let shift = (addr_u32 & 3) * 8;
                let mask = match size {
                    1 => 0xFFu32,
                    2 => 0xFFFFu32,
                    _ => 0xFFFF_FFFFu32,
                };
                let raw_val = (val as u32) & mask;
                let full_val = if size < 4 {
                    if is_destructive_fifo_reg(aligned_addr) {
                        raw_val
                    } else {
                        let cur = uc.get_data_mut().mmio.read(aligned_addr, 4);
                        (cur & !(mask << shift)) | (raw_val << shift)
                    }
                } else {
                    raw_val
                };
                uc.get_data_mut().mmio.write(aligned_addr, size, full_val);
                if (aligned_addr == QUADSPI_CR || aligned_addr == QUADSPI_AR)
                    && Self::service_dma_stream(uc, 7)
                {
                    let _ = uc.emu_stop();
                }
                true
            },
        )?;

        // Hook MMIO writes on USB OTG FS (0x5000_0000..=0x5000_FFFF)
        uc.add_mem_hook(
            HookType::MEM_WRITE,
            USB_OTG_FS_BASE,
            USB_OTG_FS_BASE + USB_OTG_FS_SIZE as u64 - 1,
            |uc, _access, addr, size, val| {
                let addr_u32 = addr as u32;
                let aligned_addr = addr_u32 & !3;
                let shift = (addr_u32 & 3) * 8;
                let mask = match size {
                    1 => 0xFFu32,
                    2 => 0xFFFFu32,
                    _ => 0xFFFF_FFFFu32,
                };
                let raw_val = (val as u32) & mask;
                let full_val = if size < 4 {
                    let cur = uc.get_data_mut().mmio.read(aligned_addr, 4);
                    (cur & !(mask << shift)) | (raw_val << shift)
                } else {
                    raw_val
                };
                uc.get_data_mut().mmio.write(aligned_addr, size, full_val);
                true
            },
        )?;

        // Bit-band reads: SRAM bit-band
        uc.add_mem_hook(
            HookType::MEM_READ,
            SRAM_BITBAND_BASE,
            SRAM_BITBAND_BASE + SRAM_BITBAND_SIZE as u64 - 1,
            |uc, _access, addr, size, _val| {
                let offset = addr - SRAM_BITBAND_BASE;
                let byte_addr = SRAM_BASE + (offset / 32);
                let bit_num = (offset % 32) / 4;
                let mut b = [0u8; 1];
                let _ = uc.mem_read(byte_addr, &mut b);
                let bit = ((b[0] >> bit_num) & 1) as u32;
                let _ = uc.mem_write(addr, &bit.to_le_bytes()[..size.min(4)]);
                true
            },
        )?;

        // Bit-band writes: SRAM bit-band
        uc.add_mem_hook(
            HookType::MEM_WRITE,
            SRAM_BITBAND_BASE,
            SRAM_BITBAND_BASE + SRAM_BITBAND_SIZE as u64 - 1,
            |uc, _access, addr, _size, val| {
                let offset = addr - SRAM_BITBAND_BASE;
                let byte_addr = SRAM_BASE + (offset / 32);
                let bit_num = (offset % 32) / 4;
                let mut b = [0u8; 1];
                let _ = uc.mem_read(byte_addr, &mut b);
                if (val & 1) != 0 {
                    b[0] |= 1 << bit_num;
                } else {
                    b[0] &= !(1 << bit_num);
                }
                let _ = uc.mem_write(byte_addr, &b);
                true
            },
        )?;

        // Bit-band reads: Peripheral bit-band
        uc.add_mem_hook(
            HookType::MEM_READ,
            PERIPH_BITBAND_BASE,
            PERIPH_BITBAND_BASE + PERIPH_BITBAND_SIZE as u64 - 1,
            |uc, _access, addr, size, _val| {
                let offset = (addr - PERIPH_BITBAND_BASE) as u32;
                let byte_addr = (PERIPH_MMIO_BASE as u32) + (offset / 32);
                let bit_num = (offset % 32) / 4;
                let reg_addr = byte_addr & !3;
                let reg_bit = (byte_addr % 4) * 8 + bit_num;

                let val = uc.get_data_mut().mmio.read(reg_addr, 4);
                let bit = ((val >> reg_bit) & 1) as u32;
                let _ = uc.mem_write(addr, &bit.to_le_bytes()[..size.min(4)]);
                true
            },
        )?;

        // Bit-band writes: Peripheral bit-band
        uc.add_mem_hook(
            HookType::MEM_WRITE,
            PERIPH_BITBAND_BASE,
            PERIPH_BITBAND_BASE + PERIPH_BITBAND_SIZE as u64 - 1,
            |uc, _access, addr, _size, val| {
                let offset = (addr - PERIPH_BITBAND_BASE) as u32;
                let byte_addr = (PERIPH_MMIO_BASE as u32) + (offset / 32);
                let bit_num = (offset % 32) / 4;
                let reg_addr = byte_addr & !3;
                let reg_bit = (byte_addr % 4) * 8 + bit_num;

                let mut cur = if is_destructive_fifo_reg(reg_addr) {
                    0
                } else {
                    uc.get_data_mut().mmio.read(reg_addr, 4)
                };
                if (val & 1) != 0 {
                    cur |= 1 << reg_bit;
                } else {
                    cur &= !(1 << reg_bit);
                }
                uc.get_data_mut().mmio.write(reg_addr, 4, cur);
                if reg_addr == (DMA2_BASE + 0x10 + 7 * 0x18)
                    && Self::service_dma_stream(uc, 7)
                {
                    let _ = uc.emu_stop();
                }
                true
            },
        )?;

        // Dynamically map unmapped memory accesses to avoid hard faults on harmless optional accesses
        uc.add_mem_hook(
            HookType::MEM_READ_UNMAPPED,
            0,
            0xFFFF_FFFF,
            |uc, _access, addr, _size, _val| {
                let page_addr = addr & !0xFFF;
                if uc.mem_map(page_addr, 0x1000, Prot::ALL).is_ok() {
                    let pc = uc.reg_read(RegisterARM::PC).unwrap_or(0);
                    crate::svd_diagnostics::log_unmapped_page_allocation(page_addr, addr, false, pc);
                    let zeros = [0u8; 4096];
                    let _ = uc.mem_write(page_addr, &zeros);
                    true
                } else {
                    false
                }
            },
        )?;

        uc.add_mem_hook(
            HookType::MEM_WRITE_UNMAPPED,
            0,
            0xFFFF_FFFF,
            |uc, _access, addr, _size, _val| {
                let page_addr = addr & !0xFFF;
                if uc.mem_map(page_addr, 0x1000, Prot::ALL).is_ok() {
                    let pc = uc.reg_read(RegisterARM::PC).unwrap_or(0);
                    crate::svd_diagnostics::log_unmapped_page_allocation(page_addr, addr, true, pc);
                    let zeros = [0u8; 4096];
                    let _ = uc.mem_write(page_addr, &zeros);
                    true
                } else {
                    false
                }
            },
        )?;

        // Hook synchronous SVC instructions to execute SVCall (vector 11) using architectural Cortex-M stacking
        uc.add_intr_hook(|uc, intno| {
            if intno == 2 {
                let _ = Self::enter_exception(uc, VECTOR_SVC);
            } else if intno == 8 {
                // EXCP_EXCEPTION_EXIT: normal return from ISR via EXC_RETURN
                let _ = uc.emu_stop();
            } else {
                let pc = uc.reg_read(RegisterARM::PC).unwrap_or(0);
                if (pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    // Normal return from ISR via EXC_RETURN trap address
                    let _ = uc.emu_stop();
                } else {
                    let _ = uc.emu_stop();
                }
            }
        })?;

        // Hook code execution to keep history of recent instruction addresses for diagnostics
        // uc.add_code_hook(0x0800_0000, 0x2003_FFFF, |uc, addr, _size| {
        //     let q = &mut uc.get_data_mut().recent_pcs;
        //     if q.len() >= 64 {
        //         q.pop_front();
        //     }
        //     q.push_back(addr as u32);
        // })?;

        let coproc = CoProcessorBusRouter::from_buffers(oled_bin, matrix_bin, kbd_bin)?;

        // Verify expected instruction bytes at qspi_patch_addr
        if let Some(patch_addr) = profile.qspi_patch_addr {
            let patch_offset = (patch_addr.saturating_sub(profile.load_addr)) as usize;
            let can_patch = patch_offset + 4 <= main_bin.len()
                && main_bin[patch_offset..patch_offset + 4] == profile.qspi_patch_prologue;

            if can_patch {
                // Patch HAL_QSPI_Receive with `movs r0, #0; bx lr`
                uc.mem_write(patch_addr as u64, &[0x00, 0x20, 0x70, 0x47])?;

                // Hook HAL_QSPI_Receive to supply ISSI NOR Flash JEDEC ID [0x9D, 0x60, 0x18]
                uc.add_code_hook(patch_addr as u64, patch_addr as u64, |uc, _addr, _size| {
                    let r1 = uc.reg_read(RegisterARM::R1).unwrap_or(0);
                    let jedec_id: [u8; 4] = [0x9D, 0x60, 0x18, 0x00];
                    let _ = uc.mem_write(r1, &jedec_id);
                })?;
            } else {
                eprintln!(
                    "[WARN] 0x{:08X} does not match expected HAL_QSPI_Receive prologue; skipping binary patch",
                    patch_addr
                );
            }
        }

        if let Some(splash_pc) = profile.boot_splash_completed_pc {
            uc.add_code_hook(splash_pc as u64, splash_pc as u64, |uc, _addr, _size| {
                if !uc.get_data().boot_splash_finished {
                    uc.get_data_mut().boot_splash_finished = true;
                    // Re-arm filter type debouncer once on boot splash exit so initial switch position (e.g. PC4 LPF) latches
                    let _ = uc.mem_write(0x2000_05A0 + 0xF5, &[0]);
                }
            })?;
        }

        // Read interrupt vectors from vector table
        let read_vec = |offset: u32| -> u32 {
            let mut b = [0u8; 4];
            if uc
                .mem_read((profile.vtor + offset * 4) as u64, &mut b)
                .is_ok()
            {
                u32::from_le_bytes(b) & !1
            } else {
                0
            }
        };

        let usart1_isr = read_vec(VECTOR_USART1);
        let usart2_isr = read_vec(VECTOR_USART2);
        let usart3_isr = read_vec(VECTOR_USART3);
        let otg_fs_isr = read_vec(VECTOR_OTG_FS);
        let dma2_stream4_isr = read_vec(VECTOR_DMA2_STREAM4);
        let dma2_stream6_isr = read_vec(VECTOR_DMA2_STREAM6);
        let dma2_stream0_isr = read_vec(VECTOR_DMA2_STREAM0);
        let exti15_10_isr = read_vec(VECTOR_EXTI15_10);
        let audio_driver_addr = profile.audio_ping_addr;

        // Unicorn/QEMU TCG workaround: In USART1 & USART2 ISRs, `it ne; ldrne r1, [r2, #4]` reads USART_DR
        // under an IT block. When Unicorn triggers a MEM_READ hook on an IT-conditional load,
        // it fails to advance ITSTATE upon resuming, corrupting the subsequent instructions in the ISR
        // and causing all RX bytes to be discarded.
        // Since the ISR is exclusively dispatched when RXNE/ORE is set, replacing `it ne` with
        // `nop` (0xBF00) makes the load unconditional and avoids the broken ITSTATE desynchronization.
        for isr in [usart1_isr, usart2_isr] {
            if isr != 0 {
                let it_addr = (isr & !1) + 0x12;
                let mut it_bytes = [0u8; 2];
                if uc.mem_read(it_addr as u64, &mut it_bytes).is_ok() && it_bytes == [0x18, 0xBF] {
                    let _ = uc.mem_write(it_addr as u64, &[0x00, 0xBF]);
                }
            }
        }

        let (coproc_req_tx, req_rx) = crossbeam_channel::bounded::<usize>(1);
        let (resp_tx, coproc_resp_rx) = crossbeam_channel::bounded::<std::time::Duration>(1);
        let worker_handle = std::thread::Builder::new()
            .name("coproc-worker".into())
            .spawn(move || {
                while let Ok(ptr_val) = req_rx.recv() {
                    if ptr_val == 0 {
                        break;
                    }
                    let coproc = unsafe { &mut *(ptr_val as *mut CoProcessorBusRouter) };
                    let t0 = std::time::Instant::now();
                    coproc.step_all(32_000);
                    let _ = resp_tx.send(t0.elapsed());
                }
            })
            .expect("failed to spawn coproc-worker thread");

        Ok(Self {
            uc,
            coproc,
            profile,
            pc: reset_handler,
            is_booted: false,
            usart1_isr,
            usart2_isr,
            usart3_isr,
            otg_fs_isr,
            dma2_stream4_isr,
            dma2_stream6_isr,
            dma2_stream0_isr,
            exti15_10_isr,
            dma_ping: true,
            audio_driver_addr,
            systick_cycle_accum: 0,
            audio_proc: crate::audio::AudioProcessor::default(),
            profiling: false,
            perf_step_mcu: std::time::Duration::ZERO,
            perf_coproc: std::time::Duration::ZERO,
            perf_render_audio: std::time::Duration::ZERO,
            perf_usart: std::time::Duration::ZERO,
            perf_fast_forward: std::time::Duration::ZERO,
            perf_systick: std::time::Duration::ZERO,
            perf_render_enter_exc: std::time::Duration::ZERO,
            perf_render_emu_start: std::time::Duration::ZERO,
            perf_render_exit_exc: std::time::Duration::ZERO,
            perf_render_dsp: std::time::Duration::ZERO,
            block_counter: 0,
            initial_pot_idx: 0,
            activity_countdown: 50,
            audio_input_countdown: 0,
            swd: crate::swd::SwdDebugger::new(),
            coproc_req_tx,
            coproc_resp_rx,
            coproc_worker_handle: Some(worker_handle),
        })
    }

    /// Convenience file-path constructor (reads files from disk and delegates to `from_buffers`).
    pub fn new<
        P1: AsRef<Path>,
        P2: AsRef<Path>,
        P3: AsRef<Path>,
        P4: AsRef<Path>,
        P5: AsRef<Path>,
    >(
        main_bin_path: P1,
        qspi_bin_path: Option<P2>,
        oled_bin_path: P3,
        matrix_bin_path: P4,
        kbd_bin_path: P5,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let main = std::fs::read(main_bin_path)?;
        let qspi = qspi_bin_path.map(std::fs::read).transpose()?;
        let oled = std::fs::read(oled_bin_path)?;
        let matrix = std::fs::read(matrix_bin_path)?;
        let kbd = std::fs::read(kbd_bin_path)?;
        Self::from_buffers(&main, qspi.as_deref(), &oled, &matrix, &kbd)
    }

    #[inline(always)]
    pub fn nvic(&self) -> &NvicController {
        &self.uc.get_data().nvic
    }

    #[inline(always)]
    pub fn nvic_mut(&mut self) -> &mut NvicController {
        &mut self.uc.get_data_mut().nvic
    }

    pub fn enter_exception(
        uc: &mut Unicorn<EmulatorContext>,
        vector_num: u32,
    ) -> Result<bool, unicorn_engine::unicorn_const::uc_error> {
        let is_nested = !uc.get_data().nvic.active_stack.is_empty();

        let isr_addr = {
            let cached = if (vector_num as usize) < uc.get_data().nvic.isr_cache.len() {
                uc.get_data().nvic.isr_cache[vector_num as usize]
            } else {
                0
            };
            if cached != 0 {
                cached
            } else {
                let vtor = uc.get_data().nvic.vtor;
                let vector_addr_loc = vtor + 4 * vector_num;
                let mut addr_bytes = [0u8; 4];
                uc.mem_read(vector_addr_loc as u64, &mut addr_bytes)?;
                let addr = u32::from_le_bytes(addr_bytes) & !1;
                let is_valid_addr = (addr >= FLASH_BASE as u32
                    && addr < (FLASH_BASE + FLASH_SIZE as u64) as u32)
                    || (addr >= SRAM_BASE as u32
                        && addr < (SRAM_BASE + SRAM_SIZE as u64) as u32)
                    || (addr >= BOOT_ALIAS_BASE as u32
                        && addr < (BOOT_ALIAS_BASE + FLASH_SIZE as u64) as u32);
                if addr == 0 || !is_valid_addr {
                    return Ok(false);
                }
                if (vector_num as usize) < uc.get_data().nvic.isr_cache.len() {
                    uc.get_data_mut().nvic.isr_cache[vector_num as usize] = addr;
                }
                addr
            }
        };

        const S_REGS: [RegisterARM; 16] = [
            RegisterARM::S0,
            RegisterARM::S1,
            RegisterARM::S2,
            RegisterARM::S3,
            RegisterARM::S4,
            RegisterARM::S5,
            RegisterARM::S6,
            RegisterARM::S7,
            RegisterARM::S8,
            RegisterARM::S9,
            RegisterARM::S10,
            RegisterARM::S11,
            RegisterARM::S12,
            RegisterARM::S13,
            RegisterARM::S14,
            RegisterARM::S15,
        ];

        let control = uc.reg_read(RegisterARM::CONTROL).unwrap_or(0) as u32;
        let was_using_psp = (control & 2) != 0 && !is_nested;
        let is_fpu_active = (control & 4) != 0;
        let frame_size: u32 = if is_fpu_active { 104 } else { 32 };

        let sp = uc.reg_read(RegisterARM::SP)? as u32;

        let r0 = uc.reg_read(RegisterARM::R0)? as u32;
        let r1 = uc.reg_read(RegisterARM::R1)? as u32;
        let r2 = uc.reg_read(RegisterARM::R2)? as u32;
        let r3 = uc.reg_read(RegisterARM::R3)? as u32;
        let r12 = uc.reg_read(RegisterARM::R12)? as u32;
        let lr = uc.reg_read(RegisterARM::LR)? as u32;
        let pc = uc.reg_read(RegisterARM::PC)? as u32;
        let cpsr = uc.reg_read(RegisterARM::CPSR)? as u32;

        let new_sp = (sp.wrapping_sub(frame_size)) & !7;
        let stacked_xpsr = (cpsr & !(1 << 9)) | ((sp & 4) << 7);

        let mut frame = [0u8; 104];
        frame[0..4].copy_from_slice(&r0.to_le_bytes());
        frame[4..8].copy_from_slice(&r1.to_le_bytes());
        frame[8..12].copy_from_slice(&r2.to_le_bytes());
        frame[12..16].copy_from_slice(&r3.to_le_bytes());
        frame[16..20].copy_from_slice(&r12.to_le_bytes());
        frame[20..24].copy_from_slice(&lr.to_le_bytes());
        frame[24..28].copy_from_slice(&pc.to_le_bytes());
        frame[28..32].copy_from_slice(&stacked_xpsr.to_le_bytes());

        if is_fpu_active && (vector_num == VECTOR_DMA2_STREAM4 || vector_num == VECTOR_DMA2_STREAM6) {
            for (i, &s_reg) in S_REGS.iter().enumerate() {
                let s_val = uc.reg_read(s_reg).unwrap_or(0) as u32;
                let off = 32 + i * 4;
                frame[off..off + 4].copy_from_slice(&s_val.to_le_bytes());
            }
            let fpscr = uc.reg_read(RegisterARM::FPSCR).unwrap_or(0) as u32;
            frame[96..100].copy_from_slice(&fpscr.to_le_bytes());
        }

        uc.mem_write(new_sp as u64, &frame[..frame_size as usize])?;

        if was_using_psp {
            uc.reg_write(RegisterARM::PSP, new_sp as u64)?;
            let mut new_ctrl = control & !2; // Clear SPSEL so Handler Mode runs on MSP
            if is_fpu_active {
                new_ctrl &= !4;
            }
            uc.reg_write(RegisterARM::CONTROL, new_ctrl as u64)?;
            let msp = uc.reg_read(RegisterARM::MSP).unwrap_or(0);
            uc.reg_write(RegisterARM::SP, msp)?;
        } else {
            uc.reg_write(RegisterARM::MSP, new_sp as u64)?;
            uc.reg_write(RegisterARM::SP, new_sp as u64)?;
            if is_fpu_active {
                uc.reg_write(RegisterARM::CONTROL, (control & !4) as u64)?;
            }
        }

        let isr_cpsr = (cpsr & !0x0600_FC00) | (1 << 24);
        uc.reg_write(RegisterARM::CPSR, isr_cpsr as u64)?;

        let exc_return = if is_nested {
            if is_fpu_active {
                0xFFFF_FFE1
            } else {
                EXC_RETURN_HANDLER_MSP
            }
        } else if was_using_psp {
            if is_fpu_active {
                0xFFFF_FFED
            } else {
                EXC_RETURN_THREAD_PSP
            }
        } else {
            if is_fpu_active {
                0xFFFF_FFE9
            } else {
                EXC_RETURN_THREAD_MSP
            }
        };
        uc.reg_write(RegisterARM::LR, exc_return as u64)?;
        uc.reg_write(RegisterARM::PC, (isr_addr | 1) as u64)?;

        let nvic = &mut uc.get_data_mut().nvic;
        nvic.active_stack.push(vector_num);
        if vector_num < 128 {
            nvic.active_vectors |= 1u128 << vector_num;
        }
        if vector_num >= 16 {
            let irq = (vector_num - 16) as usize;
            if irq < 240 {
                nvic.iabr[irq / 32] |= 1 << (irq % 32);
            }
        }
        nvic.clear_pending(vector_num);
        nvic.is_nested = nvic.active_stack.len() > 1;

        Ok(true)
    }

    pub fn exit_exception(
        uc: &mut Unicorn<EmulatorContext>,
        ret_pc: u32,
    ) -> Result<bool, unicorn_engine::unicorn_const::uc_error> {
        let use_psp = (ret_pc & 4) != 0;
        let is_extended = (ret_pc & (1 << 4)) == 0;
        let return_to_thread = (ret_pc & 8) != 0;

        let curr_sp = if use_psp {
            uc.reg_read(RegisterARM::PSP).unwrap_or(0) as u32
        } else {
            uc.reg_read(RegisterARM::SP).unwrap_or(0) as u32
        };

        let frame_size: usize = if is_extended { 104 } else { 32 };
        let mut popped = [0u8; 104];
        if uc.mem_read(curr_sp as u64, &mut popped[..frame_size]).is_err() {
            return Ok(false);
        }

        let r0 = u32::from_le_bytes(popped[0..4].try_into().unwrap());
        let r1 = u32::from_le_bytes(popped[4..8].try_into().unwrap());
        let r2 = u32::from_le_bytes(popped[8..12].try_into().unwrap());
        let r3 = u32::from_le_bytes(popped[12..16].try_into().unwrap());
        let r12 = u32::from_le_bytes(popped[16..20].try_into().unwrap());
        let lr = u32::from_le_bytes(popped[20..24].try_into().unwrap());
        let pc = u32::from_le_bytes(popped[24..28].try_into().unwrap());
        let xpsr = u32::from_le_bytes(popped[28..32].try_into().unwrap());

        uc.reg_write(RegisterARM::R0, r0 as u64)?;
        uc.reg_write(RegisterARM::R1, r1 as u64)?;
        uc.reg_write(RegisterARM::R2, r2 as u64)?;
        uc.reg_write(RegisterARM::R3, r3 as u64)?;
        uc.reg_write(RegisterARM::R12, r12 as u64)?;
        uc.reg_write(RegisterARM::LR, lr as u64)?;
        uc.reg_write(RegisterARM::PC, (pc | 1) as u64)?;
        uc.reg_write(RegisterARM::CPSR, (xpsr | 0x20) as u64)?;

        const S_REGS: [RegisterARM; 16] = [
            RegisterARM::S0,
            RegisterARM::S1,
            RegisterARM::S2,
            RegisterARM::S3,
            RegisterARM::S4,
            RegisterARM::S5,
            RegisterARM::S6,
            RegisterARM::S7,
            RegisterARM::S8,
            RegisterARM::S9,
            RegisterARM::S10,
            RegisterARM::S11,
            RegisterARM::S12,
            RegisterARM::S13,
            RegisterARM::S14,
            RegisterARM::S15,
        ];

        if is_extended {
            for (i, &s_reg) in S_REGS.iter().enumerate() {
                let off = 32 + i * 4;
                let s_val = u32::from_le_bytes(popped[off..off + 4].try_into().unwrap());
                let _ = uc.reg_write(s_reg, s_val as u64);
            }
            let fpscr = u32::from_le_bytes(popped[96..100].try_into().unwrap());
            let _ = uc.reg_write(RegisterARM::FPSCR, fpscr as u64);
        }

        let align_pad = if (xpsr & (1 << 9)) != 0 { 4 } else { 0 };
        let final_sp = curr_sp + (frame_size as u32) + align_pad;

        if use_psp {
            uc.reg_write(RegisterARM::PSP, final_sp as u64)?;
            if return_to_thread {
                uc.reg_write(RegisterARM::SP, final_sp as u64)?;
            }
        } else {
            uc.reg_write(RegisterARM::MSP, final_sp as u64)?;
            uc.reg_write(RegisterARM::SP, final_sp as u64)?;
        }

        let mut ctrl = uc.reg_read(RegisterARM::CONTROL).unwrap_or(0) as u32;
        if return_to_thread {
            if use_psp {
                ctrl |= 2;
            } else {
                ctrl &= !2;
            }
        } else {
            ctrl &= !2;
        }
        if is_extended {
            ctrl |= 4;
        } else {
            ctrl &= !4;
        }
        let _ = uc.reg_write(RegisterARM::CONTROL, ctrl as u64);

        let nvic = &mut uc.get_data_mut().nvic;
        if let Some(popped_vec) = nvic.active_stack.pop() {
            if popped_vec < 128 {
                nvic.active_vectors &= !(1u128 << popped_vec);
            }
            if popped_vec >= 16 {
                let irq = (popped_vec - 16) as usize;
                if irq < 240 {
                    nvic.iabr[irq / 32] &= !(1 << (irq % 32));
                }
            }
        }
        nvic.is_nested = !nvic.active_stack.is_empty();

        Ok(true)
    }

    pub fn service_dma_stream(uc: &mut Unicorn<EmulatorContext>, s_idx: usize) -> bool {
        let ready = {
            let dma2 = &uc.get_data().mmio.dma2;
            let stream = &dma2.streams[s_idx];
            if (stream.cr & 1) == 0 || stream.ndtr == 0 {
                false
            } else if stream.par == QUADSPI_DR {
                let qspi = &uc.get_data().mmio.qspi;
                let dmaen = (qspi.cr & (1 << 2)) != 0;
                let dir = (stream.cr >> 6) & 0x03;
                if dir == 0b00 {
                    dmaen && !qspi.dr_fifo.is_empty()
                } else if dir == 0b01 {
                    dmaen
                } else {
                    true
                }
            } else {
                true
            }
        };

        if ready {
            let (par, m0ar, ndtr, psize, msize, pinc, minc, dir) = {
                let s = &uc.get_data().mmio.dma2.streams[s_idx];
                let psize = match (s.cr >> 11) & 0x03 { 0 => 1, 1 => 2, _ => 4 };
                let msize = match (s.cr >> 13) & 0x03 { 0 => 1, 1 => 2, _ => 4 };
                let pinc = (s.cr & (1 << 9)) != 0;
                let minc = (s.cr & (1 << 10)) != 0;
                let dir = (s.cr >> 6) & 0x03;
                (s.par, s.m0ar, s.ndtr, psize, msize, pinc, minc, dir)
            };

            let mut cur_p = par;
            let mut cur_m = m0ar;
            for _ in 0..ndtr {
                if dir == 0b00 {
                    // Peripheral to memory
                    let val = if cur_p == QUADSPI_DR {
                        uc.get_data_mut().mmio.qspi.read_dr(psize)
                    } else {
                        let mut b = [0u8; 4];
                        let _ = uc.mem_read(cur_p as u64, &mut b[..psize]);
                        u32::from_le_bytes(b)
                    };
                    let _ = uc.mem_write(cur_m as u64, &val.to_le_bytes()[..msize]);
                } else if dir == 0b01 {
                    // Memory to peripheral
                    let mut b = [0u8; 4];
                    let _ = uc.mem_read(cur_m as u64, &mut b[..msize]);
                    let val = u32::from_le_bytes(b);
                    if cur_p == QUADSPI_DR {
                        uc.get_data_mut().mmio.qspi.write_dr(psize, val);
                    } else {
                        let _ = uc.mem_write(cur_p as u64, &val.to_le_bytes()[..psize]);
                    }
                } else {
                    // Memory to memory
                    let mut b = [0u8; 4];
                    let _ = uc.mem_read(cur_p as u64, &mut b[..psize]);
                    let val = u32::from_le_bytes(b);
                    let _ = uc.mem_write(cur_m as u64, &val.to_le_bytes()[..msize]);
                }

                if pinc { cur_p = cur_p.wrapping_add(psize as u32); }
                if minc { cur_m = cur_m.wrapping_add(msize as u32); }
            }

            let dma2 = &mut uc.get_data_mut().mmio.dma2;
            dma2.streams[s_idx].ndtr = 0;
            dma2.streams[s_idx].cr &= !1; // Disable stream
            dma2.set_tcif(s_idx);
            let cr = dma2.streams[s_idx].cr;
            let hisr = dma2.hisr;
            let lisr = dma2.lisr;
            let _ = uc.mem_write((DMA2_BASE + 0x10 + (s_idx as u32) * 0x18) as u64, &cr.to_le_bytes());
            let _ = uc.mem_write(DMA2_BASE as u64 + 4, &hisr.to_le_bytes());
            let _ = uc.mem_write(DMA2_BASE as u64, &lisr.to_le_bytes());

            if par == QUADSPI_DR && dir == 0b00 {
                let qspi = &mut uc.get_data_mut().mmio.qspi;
                qspi.cr &= !(1 << 2); // Clear DMAEN
                qspi.sr |= 1 << 1;    // Set TCF (Transfer Complete Flag)
                qspi.sr &= !(1 << 5); // Clear BUSY
                qspi.sr &= !(1 << 2); // Clear FTF

                let vtor = uc.get_data().nvic.vtor;
                let mut vec108_bytes = [0u8; 4];
                if uc.mem_read((vtor + 108 * 4) as u64, &mut vec108_bytes).is_ok() {
                    let isr108 = u32::from_le_bytes(vec108_bytes) & !1;
                    let mut hqspi_ptr_bytes = [0u8; 4];
                    if uc.mem_read((isr108 + 8) as u64, &mut hqspi_ptr_bytes).is_ok() {
                        let hqspi = u32::from_le_bytes(hqspi_ptr_bytes);
                        if hqspi >= SRAM_BASE as u32 && hqspi < (SRAM_BASE + SRAM_SIZE as u64) as u32 {
                            let mut state_byte = [0u8; 1];
                            if uc.mem_read((hqspi + 0x41) as u64, &mut state_byte).is_ok() {
                                if state_byte[0] == 0x22 {
                                    let _ = uc.mem_write((hqspi + 0x41) as u64, &[1u8]);
                                    let _ = uc.mem_write((hqspi + 0x40) as u64, &[0u8]);
                                    let _ = uc.mem_write((hqspi + 0x38) as u64, &[0u8; 4]);
                                    let _ = uc.mem_write((hqspi + 0x34) as u64, &[0u8; 4]);

                                    let mut hdma_ptr_bytes = [0u8; 4];
                                    if uc.mem_read((hqspi + 0x3C) as u64, &mut hdma_ptr_bytes).is_ok() {
                                        let hdma = u32::from_le_bytes(hdma_ptr_bytes);
                                        if hdma >= SRAM_BASE as u32 && hdma < (SRAM_BASE + SRAM_SIZE as u64) as u32 {
                                            let _ = uc.mem_write((hdma + 0x35) as u64, &[1u8]); // HAL_DMA_STATE_READY
                                            let _ = uc.mem_write((hdma + 0x34) as u64, &[0u8]); // Lock = 0
                                        }
                                    }

                                    let mut seq_bytes = [0u8; 4];
                                    if uc.mem_read(0x2001_DC48, &mut seq_bytes).is_ok() {
                                        let seq = u32::from_le_bytes(seq_bytes).wrapping_add(1);
                                        let _ = uc.mem_write(0x2001_DC48, &seq.to_le_bytes());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            true
        } else {
            false
        }
    }

    pub fn service_dma_transfers(&mut self) {
        // Service on-demand DMA2 streams (all except Stream 4 audio TX, Stream 6 audio RX, and Stream 0 ADC)
        for s_idx in [1, 2, 3, 5, 7] {
            Self::service_dma_stream(&mut self.uc, s_idx);
        }
    }

    pub fn evaluate_pending_interrupts(&mut self) {
        self.service_dma_transfers();

        let (
            u1_active,
            u2_active,
            dma2_s4_active,
            dma2_s0_active,
            dma2_s6_active,
            dma2_s7_active,
            qspi_active,
            usb_active,
            exti_active,
            tim5_active,
            tim8_cc_active,
            tim8_trg_active,
            tim8_up_active,
        ) = {
            let data = self.uc.get_data();
            let u1 = &data.mmio.usart1;
            let u1_active = (u1.is_txeie() && (u1.read_sr() & (1 << 7)) != 0)
                || (u1.is_rxneie() && !u1.rx_queue.is_empty());

            let u2 = &data.mmio.usart2;
            let u2_active = (u2.is_txeie() && (u2.read_sr() & (1 << 7)) != 0)
                || (u2.is_rxneie() && !u2.rx_queue.is_empty());

            let dma2 = &data.mmio.dma2;
            let dma2_s4_active = dma2.is_stream_irq_active(4);
            let dma2_s0_active = dma2.is_stream_irq_active(0);
            let dma2_s6_active = dma2.is_stream_irq_active(6);
            let dma2_s7_active = dma2.is_stream_irq_active(7);

            let qspi_active = data.mmio.qspi.is_irq_active();
            let usb_active = data.mmio.usb.is_irq_active();
            let exti = &data.mmio.exti;
            let exti_active = (exti.pr & exti.imr & ((1 << 14) | (1 << 15))) != 0;

            let tim5_active = data.mmio.timers.is_tim5_irq_active();
            let tim8_cc_active = data.mmio.timers.is_tim8_cc_irq_active();
            let tim8_trg_active = data.mmio.timers.is_tim8_trg_com_irq_active();
            let tim8_up_active = data.mmio.timers.is_tim8_up_irq_active();

            (
                u1_active,
                u2_active,
                dma2_s4_active,
                dma2_s0_active,
                dma2_s6_active,
                dma2_s7_active,
                qspi_active,
                usb_active,
                exti_active,
                tim5_active,
                tim8_cc_active,
                tim8_trg_active,
                tim8_up_active,
            )
        };

        let nvic = &mut self.uc.get_data_mut().nvic;
        if u1_active {
            nvic.set_pending(VECTOR_USART1);
        } else {
            nvic.clear_pending(VECTOR_USART1);
        }

        if u2_active {
            nvic.set_pending(VECTOR_USART2);
        } else {
            nvic.clear_pending(VECTOR_USART2);
        }

        if dma2_s4_active {
            nvic.set_pending(VECTOR_DMA2_STREAM4);
        } else {
            nvic.clear_pending(VECTOR_DMA2_STREAM4);
        }

        if dma2_s0_active {
            nvic.set_pending(VECTOR_DMA2_STREAM0);
        } else {
            nvic.clear_pending(VECTOR_DMA2_STREAM0);
        }

        if dma2_s6_active {
            nvic.set_pending(VECTOR_DMA2_STREAM6);
        } else {
            nvic.clear_pending(VECTOR_DMA2_STREAM6);
        }

        if dma2_s7_active {
            nvic.set_pending(VECTOR_DMA2_STREAM7);
        } else {
            nvic.clear_pending(VECTOR_DMA2_STREAM7);
        }

        if qspi_active {
            nvic.set_pending(VECTOR_QUADSPI);
        } else {
            nvic.clear_pending(VECTOR_QUADSPI);
        }

        if usb_active {
            nvic.set_pending(VECTOR_OTG_FS);
        } else {
            nvic.clear_pending(VECTOR_OTG_FS);
        }

        if exti_active {
            nvic.set_pending(VECTOR_EXTI15_10);
        } else {
            nvic.clear_pending(VECTOR_EXTI15_10);
        }

        if tim5_active {
            nvic.set_pending(VECTOR_TIM5);
        } else {
            nvic.clear_pending(VECTOR_TIM5);
        }

        if tim8_cc_active {
            nvic.set_pending(VECTOR_TIM8_CC);
        } else {
            nvic.clear_pending(VECTOR_TIM8_CC);
        }

        if tim8_trg_active {
            nvic.set_pending(VECTOR_TIM8_TRG_COM);
        } else {
            nvic.clear_pending(VECTOR_TIM8_TRG_COM);
        }

        if tim8_up_active {
            nvic.set_pending(VECTOR_TIM8_UP);
        } else {
            nvic.clear_pending(VECTOR_TIM8_UP);
        }
    }

    pub fn dispatch_systick(&mut self) {
        self.uc.get_data_mut().nvic.set_pending(VECTOR_SYSTICK);
        if !self.uc.get_data().nvic.is_active(VECTOR_SYSTICK) {
            let _ = Self::enter_exception(&mut self.uc, VECTOR_SYSTICK);
            let mut safety = 0;
            while self.uc.get_data().nvic.is_active(VECTOR_SYSTICK) && safety < 10 {
                let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 1000);
                let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                self.pc = new_pc;
                if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    let _ = Self::exit_exception(&mut self.uc, new_pc);
                    if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                        self.pc = restored_pc as u32;
                    }
                }
                safety += 1;
            }
        }
    }

    pub fn fast_forward_delay(&mut self, max_ticks: usize) -> usize {
        let mut ticks = 0;
        while self.profile.is_delay_spin(self.pc) && ticks < max_ticks {
            self.uc.get_data_mut().mmio.timers.step_cycles(18_000);
            self.dispatch_systick();

            // Run up to 64 instructions in the delay loop to evaluate the new tick
            let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 64);
            self.pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;

            // Handle EXC_RETURN if SysTick or another IRQ returned
            if (self.pc & 0xFFFF_0000) == 0xFFFF_0000 {
                let _ = Self::exit_exception(&mut self.uc, self.pc);
                if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                    self.pc = restored_pc as u32;
                }
                self.flush_tx_to_coproc();
            }

            // Step coprocessors proportionally: full 48k cycles if transmitting, otherwise 500 idle cycles
            let has_usart = !self.uc.get_data().tx_bytes.is_empty();
            let coproc_cycles = if has_usart { 48_000 } else { 500 };
            self.coproc.step_all(coproc_cycles);
            for b in self.coproc.drain_usart2_tx() {
                self.enqueue_usart2_rx(b);
            }
            self.service_usart1_rx();
            self.service_usart2_rx();
            self.service_usart1_tx();
            self.flush_tx_to_coproc();
            self.service_usart2_tx();

            ticks += 1;
        }
        ticks
    }

    pub fn step_mcu(&mut self, instructions: usize) -> usize {
        let mut remaining = instructions;
        let chunk_sz = 2500;

        let was_vocoder_pending = if let Some(queue_addr) = self.profile.audio_rx_queue_addr {
            if (self.uc.get_data().mmio.dma2.streams[6].cr & 1) != 0 {
                let mut ptrs = [0u8; 8];
                if self.uc.mem_read(queue_addr as u64, &mut ptrs).is_ok() {
                    let h = u32::from_le_bytes(ptrs[0..4].try_into().unwrap());
                    let t = u32::from_le_bytes(ptrs[4..8].try_into().unwrap());
                    h != t
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };

        while remaining > 0 {
            // 1. Evaluate level-triggered IRQs
            self.evaluate_pending_interrupts();

            // 2. Preempt if a pending interrupt is ready
            if let Some(vec) = self.uc.get_data().nvic.get_next_preempting_vector(&self.uc) {
                let _ = Self::enter_exception(&mut self.uc, vec);
            }

            // 3. Current PC
            let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            self.pc = curr_pc;

            // If we are spinning in a delay loop, handle idle wait
            if self.profile.is_delay_spin(curr_pc) {
                if !self.is_booted {
                    break;
                }
                let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 32);
                let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                self.pc = new_pc;
                if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    let _ = Self::exit_exception(&mut self.uc, new_pc);
                    if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                        self.pc = restored_pc as u32;
                    }
                    self.flush_tx_to_coproc();
                }
                if self.profile.is_delay_spin(self.pc) {
                    break;
                }
            }

            // 4. Run execution chunk
            let step_now = remaining.min(chunk_sz);
            let res = self.uc.emu_start((self.pc | 1) as u64, 0, 0, step_now);

            let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;

            self.pc = new_pc;
            self.sync_pending_flash_and_qspi();
            if new_pc == 0 {
                break;
            }

            if was_vocoder_pending {
                if let Some(queue_addr) = self.profile.audio_rx_queue_addr {
                    let mut ptrs = [0u8; 8];
                    if self.uc.mem_read(queue_addr as u64, &mut ptrs).is_ok() {
                        let h = u32::from_le_bytes(ptrs[0..4].try_into().unwrap());
                        let t = u32::from_le_bytes(ptrs[4..8].try_into().unwrap());
                        if h == t {
                            break;
                        }
                    }
                }
            }
            let executed = unsafe { unicorn_engine_sys::uc_get_emu_counter(self.uc.get_handle()) as usize };
            remaining = remaining.saturating_sub(executed.max(1));

            // 5. Handle EXC_RETURN
            if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                let _ = Self::exit_exception(&mut self.uc, new_pc);
                if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                    self.pc = restored_pc as u32;
                }
                self.flush_tx_to_coproc();
            }

            if let Err(e) = res {
                let sp = self.uc.reg_read(RegisterARM::SP).unwrap_or(0);
                let lr = self.uc.reg_read(RegisterARM::LR).unwrap_or(0);
                let msp = self.uc.reg_read(RegisterARM::MSP).unwrap_or(0);
                let psp = self.uc.reg_read(RegisterARM::PSP).unwrap_or(0);
                let ctrl = self.uc.reg_read(RegisterARM::CONTROL).unwrap_or(0);
                let cpsr = self.uc.reg_read(RegisterARM::CPSR).unwrap_or(0);
                let mut insn_bytes = [0u8; 16];
                let _ = self.uc.mem_read(new_pc as u64, &mut insn_bytes);
                let mut stack_bytes = [0u8; 64];
                let _ = self.uc.mem_read(sp, &mut stack_bytes);
                let stack_words: Vec<String> = stack_bytes
                    .chunks(4)
                    .map(|c| format!("0x{:08X}", u32::from_le_bytes(c.try_into().unwrap())))
                    .collect();
                let nvic = &self.uc.get_data().nvic;
                let recent = &self.uc.get_data().recent_pcs;
                let recent_hex: Vec<String> = recent.iter().map(|pc| format!("0x{:08X}", pc)).collect();
                eprintln!(
                    "[EMU ERROR] Unicorn emu_start failed at PC=0x{:08X} (curr_pc=0x{:08X}): {:?}\n\
                     Registers: SP=0x{:08X}, MSP=0x{:08X}, PSP=0x{:08X}, LR=0x{:08X}, CONTROL=0x{:X}, CPSR=0x{:08X}\n\
                     Bytes at PC: {:02X?}\n\
                     Stack [SP..SP+64]: {:?}\n\
                     NVIC: active_stack={:?}, active_vectors=0x{:X}, pending_vectors=0x{:X}\n\
                     Recent PCs (last {}): {:?}",
                    new_pc, curr_pc, e, sp, msp, psp, lr, ctrl, cpsr, insn_bytes,
                    stack_words,
                    nvic.active_stack, nvic.active_vectors, nvic.pending_vectors,
                    recent.len(), recent_hex
                );
                break;
            }
        }

        instructions - remaining
    }

    pub fn render_raw_audio_block_core(
        is_booted: bool,
        dma2_stream4_isr: u32,
        dma2_stream6_isr: u32,
        audio_driver_addr: u32,
        dma_ping: &mut bool,
        uc: &mut Unicorn<'static, EmulatorContext>,
        pc: &mut u32,
        profiling: bool,
        perf_emu_start: &mut std::time::Duration,
        perf_exit_exc: &mut std::time::Duration,
    ) -> [u8; AUDIO_BLOCK_BYTES] {
        if !is_booted || dma2_stream4_isr == 0 {
            return [0u8; AUDIO_BLOCK_BYTES];
        }

        let dma2 = &uc.get_data().mmio.dma2;
        if (dma2.streams[4].cr & 1) == 0 || dma2.streams[4].m0ar == 0 {
            return [0u8; AUDIO_BLOCK_BYTES];
        }

        let is_ping = *dma_ping;
        *dma_ping = !*dma_ping;

        let is_dbm = (dma2.streams[4].cr & (1 << 18)) != 0;
        let addr = if is_dbm {
            // In STM32 Double Buffer Mode (DBM), hardware completes buffer and triggers TCIF4 (bit 5)
            // The CT bit (bit 19) indicates the CURRENT active buffer: 0 => M0AR active, 1 => M1AR active
            let curr_target = (dma2.streams[4].cr >> 19) & 1;
            let completed_addr = if curr_target == 0 {
                dma2.audio_ping_addr().unwrap_or(audio_driver_addr)
            } else {
                dma2.audio_pong_addr().unwrap_or(audio_driver_addr + AUDIO_BLOCK_BYTES as u32)
            };
            uc.get_data_mut().mmio.dma2.set_tcif(4);
            uc.get_data_mut().mmio.dma2.streams[4].cr ^= 1 << 19; // Toggle CT
            completed_addr
        } else {
            // Single-buffer circular mode: alternate HTIF4 (half-transfer) and TCIF4 (transfer-complete)
            if is_ping {
                uc.get_data_mut().mmio.dma2.set_htif(4);
            } else {
                uc.get_data_mut().mmio.dma2.set_tcif(4);
            }
            let base_addr = uc.get_data().mmio.dma2.audio_ping_addr().unwrap_or(audio_driver_addr);
            if is_ping {
                base_addr
            } else {
                base_addr + AUDIO_BLOCK_BYTES as u32
            }
        };

        // If DMA2 Stream 6 (Audio RX / SAI2_B) is enabled, transfer digitized samples from codec ADC
        let dma2_stream6_enabled = (uc.get_data().mmio.dma2.streams[6].cr & 1) != 0;
        let dma2_stream6_m0ar = uc.get_data().mmio.dma2.streams[6].m0ar;
        if dma2_stream6_enabled && dma2_stream6_m0ar != 0 {
            let mut rx_bytes = [0u8; AUDIO_BLOCK_BYTES];
            let codec = &mut uc.get_data_mut().mmio.i2c1.codec;
            for i in 0..AUDIO_BLOCK_FRAMES {
                let (l, r) = codec.read_adc_stereo_sample();
                rx_bytes[i * 4..i * 4 + 2].copy_from_slice(&l.to_le_bytes());
                rx_bytes[i * 4 + 2..i * 4 + 4].copy_from_slice(&r.to_le_bytes());
            }
            let is_dbm6 = (uc.get_data().mmio.dma2.streams[6].cr & (1 << 18)) != 0;
            let rx_addr = if is_dbm6 {
                let curr_target = (uc.get_data().mmio.dma2.streams[6].cr >> 19) & 1;
                let target_addr = if curr_target == 0 {
                    uc.get_data().mmio.dma2.streams[6].m0ar
                } else {
                    uc.get_data().mmio.dma2.streams[6].m1ar
                };
                uc.get_data_mut().mmio.dma2.set_tcif(6);
                uc.get_data_mut().mmio.dma2.streams[6].cr ^= 1 << 19;
                target_addr
            } else {
                if is_ping {
                    uc.get_data_mut().mmio.dma2.set_htif(6);
                    dma2_stream6_m0ar
                } else {
                    uc.get_data_mut().mmio.dma2.set_tcif(6);
                    dma2_stream6_m0ar + AUDIO_BLOCK_BYTES as u32
                }
            };
            if rx_addr != 0 {
                let _ = uc.mem_write(rx_addr as u64, &rx_bytes);
            }
            let cr6 = uc.get_data().mmio.dma2.streams[6].cr;
            let _ = uc.mem_write(DMA2_BASE as u64 + 0xA0, &cr6.to_le_bytes());
        }

        // Synchronize DMA2 HISR and Stream 4 CR to Unicorn memory
        let hisr = uc.get_data().mmio.dma2.hisr;
        let _ = uc.mem_write(DMA2_BASE as u64 + 4, &hisr.to_le_bytes());
        let cr4 = uc.get_data().mmio.dma2.streams[4].cr;
        let _ = uc.mem_write(DMA2_BASE as u64 + 0x70, &cr4.to_le_bytes());

        // 1. If Stream 6 is active, execute DMA2_Stream6_IRQHandler (Audio RX / Vocoder analysis) first
        if dma2_stream6_enabled && dma2_stream6_m0ar != 0 && dma2_stream6_isr != 0 {
            uc.get_data_mut().nvic.enable_vector(VECTOR_DMA2_STREAM6);
            if !uc.get_data().nvic.is_active(VECTOR_DMA2_STREAM6) {
                let _ = Self::enter_exception(uc, VECTOR_DMA2_STREAM6);
            }
            let mut safety = 0;
            while uc.get_data().nvic.is_active(VECTOR_DMA2_STREAM6) && safety < 10 {
                let curr_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                let _ = uc.emu_start((curr_pc | 1) as u64, 0, 0, 0);
                let new_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                *pc = new_pc;
                if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    let _ = Self::exit_exception(uc, new_pc);
                    if let Ok(restored_pc) = uc.reg_read(RegisterARM::PC) {
                        *pc = restored_pc as u32;
                    }
                }
                safety += 1;
            }
        }

        // 2. Fire DMA2_Stream4_IRQHandler using stateful NVIC controller
        if dma2_stream4_isr != 0 {
            uc.get_data_mut().nvic.enable_vector(VECTOR_DMA2_STREAM4);
            if !uc.get_data().nvic.is_active(VECTOR_DMA2_STREAM4) {
                let _ = Self::enter_exception(uc, VECTOR_DMA2_STREAM4);
            }
            if profiling {
                let mut safety = 0;
                while uc.get_data().nvic.is_active(VECTOR_DMA2_STREAM4) && safety < 10 {
                    let curr_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                    let t_emu = std::time::Instant::now();
                    let _ = uc.emu_start((curr_pc | 1) as u64, 0, 0, 0);
                    *perf_emu_start += t_emu.elapsed();

                    let new_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                    *pc = new_pc;
                    if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                        let t_exit = std::time::Instant::now();
                        let _ = Self::exit_exception(uc, new_pc);
                        if let Ok(restored_pc) = uc.reg_read(RegisterARM::PC) {
                            *pc = restored_pc as u32;
                        }
                        *perf_exit_exc += t_exit.elapsed();
                    }
                    safety += 1;
                }
            } else {
                let mut safety = 0;
                while uc.get_data().nvic.is_active(VECTOR_DMA2_STREAM4) && safety < 10 {
                    let curr_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                    let _ = uc.emu_start((curr_pc | 1) as u64, 0, 0, 0);
                    let new_pc = uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                    *pc = new_pc;
                    if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                        let _ = Self::exit_exception(uc, new_pc);
                        if let Ok(restored_pc) = uc.reg_read(RegisterARM::PC) {
                            *pc = restored_pc as u32;
                        }
                    }
                    safety += 1;
                }
            }
        }

        let mut block = [0u8; AUDIO_BLOCK_BYTES];
        let _ = uc.mem_read(addr as u64, &mut block);
        block
    }

    pub fn render_raw_audio_block(&mut self) -> [u8; AUDIO_BLOCK_BYTES] {
        Self::render_raw_audio_block_core(
            self.is_booted,
            self.dma2_stream4_isr,
            self.dma2_stream6_isr,
            self.audio_driver_addr,
            &mut self.dma_ping,
            &mut self.uc,
            &mut self.pc,
            self.profiling,
            &mut self.perf_render_emu_start,
            &mut self.perf_render_exit_exc,
        )
    }

    pub fn render_audio_block_core(
        is_booted: bool,
        dma2_stream4_isr: u32,
        dma2_stream6_isr: u32,
        audio_driver_addr: u32,
        dma_ping: &mut bool,
        uc: &mut Unicorn<'static, EmulatorContext>,
        pc: &mut u32,
        audio_proc: &mut crate::audio::AudioProcessor,
        profiling: bool,
        perf_emu_start: &mut std::time::Duration,
        perf_exit_exc: &mut std::time::Duration,
        perf_dsp: &mut std::time::Duration,
    ) -> [u8; AUDIO_BLOCK_BYTES] {
        let mut block = Self::render_raw_audio_block_core(
            is_booted,
            dma2_stream4_isr,
            dma2_stream6_isr,
            audio_driver_addr,
            dma_ping,
            uc,
            pc,
            profiling,
            perf_emu_start,
            perf_exit_exc,
        );

        // Physical SEM Cutoff CV from TIM3 CCR3 (fine LSB) and CCR4 (coarse MSB)
        let tim3 = &uc.get_data().mmio.timers.timers[2];
        let cutoff_16b = (((tim3.ccr4 & 0xFF) as u16) << 8) | ((tim3.ccr3 & 0xFF) as u16);
        if cutoff_16b > 0 {
            audio_proc.set_cutoff_raw_16b(cutoff_16b);
        }

        // Physical SEM Resonance CV from DAC1 Channel 2 (PA5, DHR12R2)
        let dac = &uc.get_data().mmio.dac;
        let resonance_12b = (dac.dhr12r2 & 0x0FFF) as u16;
        if resonance_12b > 0 {
            audio_proc.set_resonance_raw_12b(resonance_12b);
        }

        // Physical VCA Gain CV from TIM14 CCR1 (PA7, 13-bit PWM DAC)
        let tim14 = &uc.get_data().mmio.timers.timers[13];
        if tim14.arr > 0 {
            let gain = (tim14.ccr1 as f32) / (tim14.arr as f32);
            audio_proc.set_vca_gain(gain.clamp(0.0, 1.0));
        }

        // Update physical SEM filter mode from hardware GPIO pins (PD4=LP, PD5=HP, PA15=BP)
        let mode = uc.get_data().mmio.gpio.filter_mode;
        audio_proc.set_filter_mode(mode);

        // Process audio through 12dB/oct SEM State-Variable Filter and slew-limited VCA
        if profiling {
            let t = std::time::Instant::now();
            audio_proc.process_block(&mut block);
            *perf_dsp += t.elapsed();
        } else {
            audio_proc.process_block(&mut block);
        }

        block
    }

    pub fn render_audio_block(&mut self) -> [u8; AUDIO_BLOCK_BYTES] {
        Self::render_audio_block_core(
            self.is_booted,
            self.dma2_stream4_isr,
            self.dma2_stream6_isr,
            self.audio_driver_addr,
            &mut self.dma_ping,
            &mut self.uc,
            &mut self.pc,
            &mut self.audio_proc,
            self.profiling,
            &mut self.perf_render_emu_start,
            &mut self.perf_render_exit_exc,
            &mut self.perf_render_dsp,
        )
    }

    pub fn service_usart1_tx(&mut self) {
        if self.usart1_isr == 0 || !self.uc.get_data().mmio.usart1.is_txeie() {
            return;
        }
        self.uc.get_data_mut().nvic.enable_vector(VECTOR_USART1);
        for _ in 0..64 {
            if !self.uc.get_data().mmio.usart1.is_txeie() {
                break;
            }
            self.uc.get_data_mut().nvic.set_pending(VECTOR_USART1);
            if let Some(vec) = self.uc.get_data().nvic.get_next_preempting_vector(&self.uc) {
                if vec == VECTOR_USART1 {
                    let _ = Self::enter_exception(&mut self.uc, vec);
                }
            }
            let mut safety = 0;
            while self.uc.get_data().nvic.is_active(VECTOR_USART1) && safety < 20 {
                self.step_mcu(2500);
                safety += 1;
            }
        }
    }

    pub fn service_usart1_rx(&mut self) {
        if self.usart1_isr == 0 || !self.uc.get_data().mmio.usart1.is_rxneie() {
            return;
        }
        self.uc.get_data_mut().nvic.enable_vector(VECTOR_USART1);
        let mut loops = 0;
        while !self.uc.get_data().mmio.usart1.rx_queue.is_empty() && loops < 64 {
            self.uc.get_data_mut().nvic.set_pending(VECTOR_USART1);
            if !self.uc.get_data().nvic.is_active(VECTOR_USART1) {
                let _ = Self::enter_exception(&mut self.uc, VECTOR_USART1);
            }
            let mut safety = 0;
            while self.uc.get_data().nvic.is_active(VECTOR_USART1) && safety < 10 {
                let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 5000);
                let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                self.pc = new_pc;
                if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    let _ = Self::exit_exception(&mut self.uc, new_pc);
                    if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                        self.pc = restored_pc as u32;
                    }
                }
                safety += 1;
            }
            loops += 1;
        }
    }

    pub fn drain_midi_out(&mut self) -> Vec<u8> {
        self.uc.get_data_mut().usart1_tx_bytes.drain(..).collect()
    }

    pub fn enqueue_usart1_rx(&mut self, b: u8) {
        self.uc.get_data_mut().mmio.usart1.push_rx(b);
    }

    pub fn service_usart2_tx(&mut self) {
        if self.usart2_isr == 0 || !self.uc.get_data().mmio.usart2.is_txeie() {
            return;
        }
        self.uc.get_data_mut().nvic.enable_vector(VECTOR_USART2);
        for _ in 0..64 {
            if !self.uc.get_data().mmio.usart2.is_txeie() {
                break;
            }
            self.uc.get_data_mut().nvic.set_pending(VECTOR_USART2);
            if !self.uc.get_data().nvic.is_active(VECTOR_USART2) {
                let _ = Self::enter_exception(&mut self.uc, VECTOR_USART2);
            }
            let mut safety = 0;
            while self.uc.get_data().nvic.is_active(VECTOR_USART2) && safety < 10 {
                let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 5000);
                let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                self.pc = new_pc;
                if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    let _ = Self::exit_exception(&mut self.uc, new_pc);
                    if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                        self.pc = restored_pc as u32;
                    }
                    self.flush_tx_to_coproc();
                }
                safety += 1;
            }
        }
    }

    pub fn service_usart2_rx(&mut self) {
        if self.usart2_isr == 0
            || !self.uc.get_data().mmio.usart2.is_rxneie()
            || self.uc.get_data().mmio.usart2.rx_queue.is_empty()
        {
            return;
        }
        self.uc.get_data_mut().nvic.enable_vector(VECTOR_USART2);
        self.uc.get_data_mut().nvic.set_pending(VECTOR_USART2);
        if !self.uc.get_data().nvic.is_active(VECTOR_USART2) {
            let _ = Self::enter_exception(&mut self.uc, VECTOR_USART2);
        }

        let mut loops = 0;
        while self.uc.get_data().nvic.is_active(VECTOR_USART2) && loops < 64 {
            let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 5000);
            let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
            self.pc = new_pc;
            if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                self.flush_tx_to_coproc();
                // Architectural Cortex-M tail-chaining: if more bytes are pending in the RX FIFO,
                // re-vector directly into the ISR without redundant stack frame pop and push.
                if !self.uc.get_data().mmio.usart2.rx_queue.is_empty() {
                    let _ = self.uc.reg_write(RegisterARM::PC, (self.usart2_isr | 1) as u64);
                    let _ = self.uc.reg_write(RegisterARM::LR, new_pc as u64);
                } else {
                    let _ = Self::exit_exception(&mut self.uc, new_pc);
                    if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                        self.pc = restored_pc as u32;
                    }
                    break;
                }
            } else {
                break;
            }
            loops += 1;
        }
    }

    pub fn flush_tx_to_coproc(&mut self) {
        let tx_bytes: Vec<u8> = self.uc.get_data_mut().tx_bytes.drain(..).collect();
        for b in tx_bytes {
            self.coproc.feed_usart2_byte(b);
        }
    }

    pub fn enqueue_usart2_rx(&mut self, b: u8) {
        self.uc.get_data_mut().mmio.usart2.push_rx(b);
    }

    pub fn enqueue_midi_byte(&mut self, byte: u8) {
        self.uc.get_data_mut().mmio.usart1.push_rx(byte);
        if self.usart1_isr != 0 {
            self.uc.get_data_mut().nvic.enable_vector(VECTOR_USART1);
            self.uc.get_data_mut().nvic.set_pending(VECTOR_USART1);
        }
    }

    pub fn enqueue_midi_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.enqueue_midi_byte(b);
        }
    }

    pub fn service_usart3_rx(&mut self) {
        if self.usart3_isr == 0 || !self.uc.get_data().mmio.usart3.is_rxneie() {
            return;
        }
        self.uc.get_data_mut().nvic.enable_vector(VECTOR_USART3);
        let mut loops = 0;
        while !self.uc.get_data().mmio.usart3.rx_queue.is_empty() && loops < 64 {
            self.uc.get_data_mut().nvic.set_pending(VECTOR_USART3);
            if !self.uc.get_data().nvic.is_active(VECTOR_USART3) {
                let _ = Self::enter_exception(&mut self.uc, VECTOR_USART3);
            }
            let mut safety = 0;
            while self.uc.get_data().nvic.is_active(VECTOR_USART3) && safety < 10 {
                let curr_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                let _ = self.uc.emu_start((curr_pc | 1) as u64, 0, 0, 5000);
                let new_pc = self.uc.reg_read(RegisterARM::PC).unwrap_or(0) as u32;
                self.pc = new_pc;
                if (new_pc & 0xFFFF_0000) == 0xFFFF_0000 {
                    let _ = Self::exit_exception(&mut self.uc, new_pc);
                    if let Ok(restored_pc) = self.uc.reg_read(RegisterARM::PC) {
                        self.pc = restored_pc as u32;
                    }
                }
                safety += 1;
            }
            loops += 1;
        }
    }

    pub fn enqueue_usb_midi_packet(&mut self, packet: [u8; 4]) {
        self.uc.get_data_mut().mmio.usb.enqueue_rx_packet(packet);
        if self.otg_fs_isr != 0 {
            self.uc.get_data_mut().nvic.enable_vector(VECTOR_OTG_FS);
            self.uc.get_data_mut().nvic.set_pending(VECTOR_OTG_FS);
        }
    }

    pub fn drain_usb_midi_packets(&mut self) -> Vec<[u8; 4]> {
        self.uc.get_data_mut().mmio.usb.drain_tx_packets()
    }

    pub fn service_usb_rx(&mut self) {
        if self.otg_fs_isr == 0 || !self.uc.get_data().mmio.usb.is_irq_active() {
            return;
        }
        self.uc.get_data_mut().nvic.enable_vector(VECTOR_OTG_FS);
        let mut loops = 0;
        while !self.uc.get_data().mmio.usb.rx_packets.is_empty() && loops < 64 {
            self.uc.get_data_mut().nvic.set_pending(VECTOR_OTG_FS);
            if let Some(vec) = self.uc.get_data().nvic.get_next_preempting_vector(&self.uc) {
                if vec == VECTOR_OTG_FS {
                    let _ = Self::enter_exception(&mut self.uc, vec);
                }
            }
            let mut safety = 0;
            while self.uc.get_data().nvic.is_active(VECTOR_OTG_FS) && safety < 20 {
                self.step_mcu(2500);
                safety += 1;
            }
            loops += 1;
        }
    }

    pub fn flush_qspi_to_disk<P: AsRef<Path>>(&self, path: P) -> std::io::Result<()> {
        let p = path.as_ref();
        let tmp_path = p.with_extension("tmp");
        std::fs::write(&tmp_path, &self.uc.get_data().mmio.qspi.flash_memory)?;
        std::fs::rename(&tmp_path, p)?;
        Ok(())
    }

    pub fn load_qspi_from_disk<P: AsRef<Path>>(&mut self, path: P) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        let mut flash = self.uc.get_data().mmio.qspi.flash_memory.clone();
        if flash.iter().all(|&b| b == 0xFF) {
            flash = crate::factory_data::generate_factory_qspi_image();
        }
        apply_qspi_data(&mut flash, &data);
        self.uc.get_data_mut().mmio.qspi.load_flash_image(&flash);
        let _ = self.uc.mem_write(QSPI_FLASH_BASE, &flash);
        let _ = self.uc.ctl_remove_cache(QSPI_FLASH_BASE, QSPI_FLASH_BASE + QSPI_FLASH_SIZE as u64);
        Ok(())
    }

    pub fn is_qspi_dirty(&self) -> bool {
        self.uc.get_data().mmio.qspi.is_dirty()
    }

    pub fn clear_qspi_dirty(&mut self) {
        self.uc.get_data_mut().mmio.qspi.clear_dirty();
    }

    /// Configure whether Unicorn JIT VFP Fast-Path (hardware single-precision floating-point acceleration
    /// for host ARM64 and x86-64) is enabled or disabled.
    pub fn set_vfp_fast_path(&mut self, enabled: bool) {
        unsafe {
            unicorn_engine_sys::uc_set_vfp_fastpath(if enabled { 1 } else { 0 });
        }
    }

    /// Query whether Unicorn JIT VFP Fast-Path is currently enabled.
    pub fn is_vfp_fast_path_enabled(&self) -> bool {
        unsafe {
            unicorn_engine_sys::uc_get_vfp_fastpath() != 0
        }
    }

    pub fn sync_pending_flash_and_qspi(&mut self) {
        if !self.uc.get_data().mmio.qspi.pending_sync.is_empty() {
            let syncs: Vec<(usize, usize)> = std::mem::take(&mut self.uc.get_data_mut().mmio.qspi.pending_sync);
            for (start, len) in syncs {
                if start + len <= QSPI_FLASH_SIZE {
                    let bytes = self.uc.get_data().mmio.qspi.flash_memory[start..start + len].to_vec();
                    let _ = self.uc.mem_write(
                        QSPI_FLASH_BASE + start as u64,
                        &bytes,
                    );
                    let _ = self.uc.ctl_remove_cache(
                        QSPI_FLASH_BASE + start as u64,
                        QSPI_FLASH_BASE + (start + len) as u64,
                    );
                }
            }
        }
        if let Some(sector) = self.uc.get_data_mut().mmio.flash.pending_erase_sector.take() {
            if let Some((start, end)) =
                crate::peripherals::flash::FlashPeripheral::sector_address_range(sector)
            {
                let len = (end - start + 1) as usize;
                let ff_buf = vec![0xFFu8; len];
                let _ = self.uc.mem_write(start as u64, &ff_buf);
                let _ = self.uc.ctl_remove_cache(start as u64, end as u64);
            }
        }
    }

    pub fn flush_coproc_to_usart(&mut self) {
        for _ in 0..4 {
            self.coproc.step_all(50_000);
            let drained = self.coproc.drain_usart2_tx();
            for b in drained {
                self.enqueue_usart2_rx(b);
            }
        }
        self.service_usart1_rx();
        self.service_usart2_rx();
        self.service_usart3_rx();
    }

    pub fn boot(&mut self, instructions: usize) {
        let max_cycles = 1_200_000_000;
        let mut rem = instructions.min(max_cycles);
        let slice_sz = 18_000;
        let mut curr_pc = self.pc;

        let mut time_step_mcu = std::time::Duration::ZERO;
        let mut time_fast_forward = std::time::Duration::ZERO;
        let mut time_systick = std::time::Duration::ZERO;
        let mut time_usart = std::time::Duration::ZERO;
        let mut time_coproc = std::time::Duration::ZERO;
        let mut loop_count = 0;
        let t_boot = std::time::Instant::now();

        while rem > 0 {
            loop_count += 1;
            let is_generic_booted = self.profile.boot_superloop_range.is_none()
                && self.profile.boot_splash_completed_pc.is_none()
                && ((self.uc.get_data().mmio.dma2.streams[4].cr & 1) != 0 || rem <= instructions.saturating_sub(2_000_000));

            if self.profile.is_delay_spin(self.pc) {
                let t = std::time::Instant::now();
                self.fast_forward_delay(100);
                time_fast_forward += t.elapsed();
                if self.uc.get_data().boot_splash_finished || self.profile.is_boot_superloop(self.pc) || is_generic_booted {
                    break;
                }
            }

            let step_now = rem.min(slice_sz);
            let t = std::time::Instant::now();
            let stepped = self.step_mcu(step_now);
            time_step_mcu += t.elapsed();
            curr_pc = self.pc;
            rem -= if stepped > 0 { stepped } else { step_now };

            // Check if boot splash finished and preset UI / superloop is reached
            if self.uc.get_data().boot_splash_finished || self.profile.is_boot_superloop(curr_pc) || is_generic_booted {
                break;
            }

            // Dispatch SysTick to advance ticks at ~10 kHz (every 18,000 cycles at 180 MHz)
            self.systick_cycle_accum += (if stepped > 0 { stepped } else { step_now }) as u32;
            let t = std::time::Instant::now();
            while self.systick_cycle_accum >= 18_000 {
                self.systick_cycle_accum -= 18_000;
                self.dispatch_systick();
            }
            time_systick += t.elapsed();

            let t = std::time::Instant::now();
            self.service_usart1_tx();
            self.service_usart1_rx();

            self.flush_tx_to_coproc();
            self.service_usart2_tx();
            self.service_usart2_rx();
            time_usart += t.elapsed();

            let t = std::time::Instant::now();
            let coproc_cycles = (step_now * 4) / 15;
            self.coproc.step_all(coproc_cycles.max(1));

            for b in self.coproc.drain_usart2_tx() {
                self.enqueue_usart2_rx(b);
            }
            self.service_usart1_rx();
            self.service_usart2_rx();
            time_coproc += t.elapsed();

            curr_pc = self.pc;
            if self.uc.get_data().boot_splash_finished || self.profile.is_boot_superloop(curr_pc) || is_generic_booted {
                break;
            }
        }

        eprintln!(
            "[BOOT BREAKDOWN] Total: {:?}, loops: {}, rem: {}, step_mcu: {:?}, fast_forward: {:?}, systick: {:?}, usart: {:?}, coproc: {:?}",
            t_boot.elapsed(), loop_count, rem, time_step_mcu, time_fast_forward, time_systick, time_usart, time_coproc
        );

        self.pc = curr_pc;
        self.is_booted = true;

        // Drain any remaining frames
        self.service_usart1_tx();
        self.flush_tx_to_coproc();
        self.service_usart2_tx();
        for _ in 0..25 {
            self.coproc.step_all(40_000);
            for b in self.coproc.drain_usart2_tx() {
                self.enqueue_usart2_rx(b);
            }
            self.service_usart1_rx();
            self.service_usart2_rx();
        }
        if let Ok(new_pc) = self.uc.reg_read(RegisterARM::PC) {
            self.pc = new_pc as u32;
        }
    }

    /// Single-threaded deterministic 2-slice micro-quantum co-scheduler (Section 17.3)
    /// Runs STM32F446 and the 3x pure-Rust STM32F0 coprocessors in lockstep virtual time
    /// for one physical 32-sample audio block (32 / 48 kHz = 666.67 µs).
    pub fn step_audio_block_quantum(&mut self) -> [u8; AUDIO_BLOCK_BYTES] {
        if !self.is_booted {
            return [0u8; AUDIO_BLOCK_BYTES];
        }

        let mut usart2_ring_pending = false;
        if let Some(queue_addr) = self.profile.usart2_rx_queue_addr {
            if self.is_booted {
                let mut ptrs = [0u8; 8];
                if self.uc.mem_read(queue_addr as u64, &mut ptrs).is_ok() {
                    let h = u32::from_le_bytes(ptrs[0..4].try_into().unwrap());
                    let t = u32::from_le_bytes(ptrs[4..8].try_into().unwrap());
                    if h != t {
                        usart2_ring_pending = true;
                    }
                }
            }
        }

        let has_midi_or_usb = !self.uc.get_data().mmio.usart1.rx_queue.is_empty()
            || !self.uc.get_data().mmio.usb.rx_packets.is_empty();
        let has_coproc_events = !self.uc.get_data().mmio.usart2.rx_queue.is_empty()
            || !self.uc.get_data().tx_bytes.is_empty()
            || self.uc.get_data().mmio.usart2.is_txeie()
            || usart2_ring_pending;
        let mut vocoder_pending = false;
        if let Some(queue_addr) = self.profile.audio_rx_queue_addr {
            if (self.uc.get_data().mmio.dma2.streams[6].cr & 1) != 0 {
                let mut ptrs = [0u8; 8];
                if self.uc.mem_read(queue_addr as u64, &mut ptrs).is_ok() {
                    let h = u32::from_le_bytes(ptrs[0..4].try_into().unwrap());
                    let t = u32::from_le_bytes(ptrs[4..8].try_into().unwrap());
                    if h != t {
                        vocoder_pending = true;
                    }
                }
            }
        }

        let is_audio_rx_active = vocoder_pending
            || (self.profile.audio_rx_queue_addr.is_none()
                && (self.uc.get_data().mmio.dma2.streams[6].cr & 1) != 0
                && (self.audio_input_countdown > 0
                    || self.uc.get_data().mmio.i2c1.codec.input_buffer_len() > 0));

        if self.audio_input_countdown > 0 {
            self.audio_input_countdown -= 1;
        }

        if has_midi_or_usb || has_coproc_events {
            self.activity_countdown = self.activity_countdown.max(60);
        } else if self.activity_countdown > 0 {
            self.activity_countdown -= 1;
        }

        let mcu_step_budget = if is_audio_rx_active {
            20_000
        } else if has_midi_or_usb {
            5000
        } else if self.activity_countdown > 0 {
            5000
        } else if has_coproc_events {
            600
        } else if self.profile.is_delay_spin(self.pc) {
            32
        } else if !self.uc.get_data().boot_splash_finished
            || self.initial_pot_idx < crate::coproc::matrix::AUTHENTIC_POTENTIOMETERS.len()
        {
            5000
        } else {
            250
        };

        self.block_counter += 1;

        // Autonomous physical potentiometer baseline scan (STM32F0 ADC sequential scan)
        // Dispatches 1 potentiometer every 24 audio blocks (~32 ms) after boot splash completion
        // until all 12 baselines are initialized. Paced to prevent overflowing the firmware's 64-byte USART2 ring buffer.
        if (self.uc.get_data().boot_splash_finished || self.block_counter >= 500)
            && (self.block_counter % 24) == 0
            && self.initial_pot_idx < crate::coproc::matrix::AUTHENTIC_POTENTIOMETERS.len()
        {
            let (group, id) =
                crate::coproc::matrix::AUTHENTIC_POTENTIOMETERS[self.initial_pot_idx];
            let val = if group == 0x07 && id == 0x04 { 4095 } else { 2048 };
            self.coproc.set_potentiometer(group, id, val);
            self.initial_pot_idx += 1;
        }

        // Service any pending external inputs (MIDI, USB) before background MCU execution
        self.service_usart1_rx();
        self.service_usart2_rx();
        self.service_usart3_rx();

        // Advance hardware timers by 120,000 cycles (32 samples @ 48 kHz = 666.67 µs)
        self.uc.get_data_mut().mmio.timers.step_cycles(120_000);

        let timer_irq_active = self.uc.get_data().mmio.timers.is_tim5_irq_active()
            || self.uc.get_data().mmio.timers.is_tim8_cc_irq_active();
        let effective_step_budget = if timer_irq_active {
            mcu_step_budget.max(2500)
        } else {
            mcu_step_budget
        };

        if self.profiling {
            // 1. Step STM32F446 in Unicorn (background event servicing)
            let t = std::time::Instant::now();
            self.step_mcu(effective_step_budget);
            self.perf_step_mcu += t.elapsed();

            // 2. Flush USART2 TX buffers to coprocessors
            let t = std::time::Instant::now();
            self.service_usart1_tx();
            self.flush_tx_to_coproc();
            self.service_usart2_tx();
            self.perf_usart += t.elapsed();
        } else {
            self.step_mcu(effective_step_budget);
            if self.pc == 0 {
                return [0u8; AUDIO_BLOCK_BYTES];
            }
            self.service_usart1_tx();
            self.flush_tx_to_coproc();
            self.service_usart2_tx();
        }

        // 3. Concurrently step 3x STM32F0 coprocessors (32,000 cycles) on persistent worker and render audio block via DMA2 Stream 4 ISR
        let t_send = if self.profiling { Some(std::time::Instant::now()) } else { None };
        let coproc_ptr = &mut self.coproc as *mut CoProcessorBusRouter as usize;
        let _ = self.coproc_req_tx.send(coproc_ptr);
        let _dur_send = t_send.map(|t| t.elapsed());

        let t_audio = if self.profiling {
            Some(std::time::Instant::now())
        } else {
            None
        };
        let blk = Self::render_audio_block_core(
            self.is_booted,
            self.dma2_stream4_isr,
            self.dma2_stream6_isr,
            self.audio_driver_addr,
            &mut self.dma_ping,
            &mut self.uc,
            &mut self.pc,
            &mut self.audio_proc,
            self.profiling,
            &mut self.perf_render_emu_start,
            &mut self.perf_render_exit_exc,
            &mut self.perf_render_dsp,
        );
        let audio_dur = t_audio.map(|t| t.elapsed());

        let t_recv = if self.profiling { Some(std::time::Instant::now()) } else { None };
        let coproc_dur = self.coproc_resp_rx.recv().unwrap_or(std::time::Duration::ZERO);
        let _dur_recv = t_recv.map(|t| t.elapsed());

        if self.profiling {
            self.perf_coproc += coproc_dur;
            if let Some(dur) = audio_dur {
                self.perf_render_audio += dur;
            }

            // 4. Transfer response frames back into STM32F446 RX buffers
            let t = std::time::Instant::now();
            let drained = self.coproc.drain_usart2_tx();
            for b in drained {
                self.enqueue_usart2_rx(b);
            }
            self.service_usart1_rx();
            self.service_usart2_rx();
            self.service_usart3_rx();
            self.perf_usart += t.elapsed();

            // 5. Advance SysTick timer naturally at 10 kHz (every 18,000 cycles at 180 MHz = 100 µs)
            self.systick_cycle_accum += 120_000;
            while self.systick_cycle_accum >= 18_000 {
                self.systick_cycle_accum -= 18_000;
                let t = std::time::Instant::now();
                self.dispatch_systick();
                self.perf_systick += t.elapsed();
            }
        } else {
            let drained = self.coproc.drain_usart2_tx();
            for b in drained {
                self.enqueue_usart2_rx(b);
            }
            self.service_usart1_rx();
            self.service_usart2_rx();
            self.service_usart3_rx();

            self.systick_cycle_accum += 120_000;
            while self.systick_cycle_accum >= 18_000 {
                self.systick_cycle_accum -= 18_000;
                self.dispatch_systick();
            }
        }

        let _ = if self.profiling { Some(std::time::Instant::now()) } else { None };
        self.sync_pending_flash_and_qspi();
        blk
    }

    pub fn step_scheduler(&mut self, count: usize) {
        if !self.is_booted {
            return;
        }

        self.service_usart1_rx();
        self.service_usart2_rx();
        self.service_usart3_rx();

        let slice_size = 2500;
        let mut remaining = count;
        while remaining > 0 {
            if self.profile.is_delay_spin(self.pc) {
                self.fast_forward_delay(10);
            }

            let step_now = remaining.min(slice_size);
            let stepped = self.step_mcu(step_now);
            let executed_cycles = (if stepped > 0 { stepped } else { step_now }) as u32;
            remaining -= if stepped > 0 { stepped } else { step_now };

            self.uc.get_data_mut().mmio.timers.step_cycles(executed_cycles);
            // Dispatch SysTick to advance FreeRTOS ticks naturally using unified systick_cycle_accum
            self.systick_cycle_accum += executed_cycles;
            while self.systick_cycle_accum >= 18_000 {
                self.systick_cycle_accum -= 18_000;
                self.dispatch_systick();
            }

            self.service_usart1_tx();
            self.service_usart1_rx();

            self.flush_tx_to_coproc();
            self.service_usart2_tx();
            self.service_usart2_rx();
            self.service_usart3_rx();
        }

        self.coproc.step_all(20_000);
        let drained = self.coproc.drain_usart2_tx();
        for b in drained {
            self.enqueue_usart2_rx(b);
        }
        self.service_usart1_rx();
        self.service_usart2_rx();
        self.service_usart3_rx();
    }

    // --- Physical Front Panel Control Methods ---

    pub fn set_microswitch(&mut self, name: &str, is_pressed: bool) {
        self.activity_countdown = 150;
        if name == "filter_type" {
            if is_pressed {
                // Advance the physical 3-position slide switch on GPIOC:
                // PC4 = LowPass, PC9 = BandPass, PC12 = HighPass
                let gpioc_idr = self.uc.get_data().mmio.gpio.ports[2].idr;
                let next_idr = if (gpioc_idr & (1 << 4)) != 0 {
                    (gpioc_idr & !(1 << 4)) | (1 << 9)
                } else if (gpioc_idr & (1 << 9)) != 0 {
                    (gpioc_idr & !(1 << 9)) | (1 << 12)
                } else {
                    (gpioc_idr & !((1 << 9) | (1 << 12))) | (1 << 4)
                };
                self.uc.get_data_mut().mmio.gpio.ports[2].idr = next_idr;
                let _ = self.uc.mem_write(0x4002_0810, &next_idr.to_le_bytes());
            }
        }
        self.coproc.set_microswitch(name, is_pressed);
    }

    pub fn set_raw_pin(&mut self, port: char, pin: u8, group: u8, is_pressed: bool) {
        let port_upper = port.to_ascii_uppercase();
        if port_upper == 'C' && (pin == 4 || pin == 9 || pin == 12) && group == 0x01 {
            if is_pressed {
                let mask = 1 << pin;
                let other_masks = (1 << 4) | (1 << 9) | (1 << 12);
                let current_idr = self.uc.get_data().mmio.gpio.ports[2].idr;
                let next_idr = (current_idr & !other_masks) | mask;
                self.uc.get_data_mut().mmio.gpio.ports[2].idr = next_idr;
                let _ = self.uc.mem_write(0x4002_0810, &next_idr.to_le_bytes());
            }
            return;
        }
        if port_upper == 'D' {
            let mask = 1 << (pin & 15);
            let current_idr = self.uc.get_data().mmio.gpio.ports[3].idr;
            let next_idr = if is_pressed {
                current_idr | mask
            } else {
                current_idr & !mask
            };
            self.uc.get_data_mut().mmio.gpio.ports[3].idr = next_idr;
            let _ = self.uc.mem_write(0x4002_0C10, &next_idr.to_le_bytes());
        }
        if let Some(c) = crate::coproc::matrix::FRONT_PANEL_CONTACTS.iter().find(|c| {
            (c.port == port_upper && c.pin == pin) || (c.group == group && (c.pin == pin || c.id == pin))
        }) {
            self.set_microswitch(c.name, is_pressed);
        } else {
            self.coproc.set_pin_level(port_upper, pin, is_pressed);
        }
    }

    pub fn step_raw_encoder(&mut self, port: char, pin: u8, group: u8, delta: i16) {
        let port_upper = port.to_ascii_uppercase();
        if (port_upper == 'A' && pin == 4) || (port_upper == 'C' && pin == 10) || (group == 0x08 && (pin == 0x00 || pin == 0x01 || pin == 0x04)) {
            self.step_preset_encoder(delta);
        } else if (port_upper == 'C' && pin == 6) || (group == 0x09 && (pin == 0x00 || pin == 0x01 || pin == 0x05)) {
            self.step_matrix_encoder(delta);
        } else if (port_upper == 'A' && pin == 8) || (group == 0x00 && pin == 0x00) || (group == 0x01 && pin == 0x00) {
            self.step_type_encoder(delta);
        } else if (port_upper == 'A' && pin == 1) || (group == 0x00 && pin == 0x01) {
            self.step_wave_encoder(delta);
        } else if (port_upper == 'A' && pin == 4) || (group == 0x00 && pin == 0x03) {
            self.step_timbre_encoder(delta);
        } else if (port_upper == 'A' && pin == 3) || (group == 0x00 && pin == 0x05) {
            self.step_shape_encoder(delta);
        } else if port_upper == 'C' && pin == 7 {
            self.step_rate_encoder(delta);
        } else if port_upper == 'B' && pin == 0 {
            self.step_lfo_rate_encoder(delta);
        } else {
            let cmd = 0x0F;
            self.coproc.step_encoder_raw(cmd, group, pin, delta);
        }
    }

    pub fn set_filter_mode_switch(&mut self, mode: SemFilterMode) {
        let mut gpioc_idr = self.uc.get_data().mmio.gpio.ports[2].idr;
        gpioc_idr &= !((1 << 4) | (1 << 9) | (1 << 12));
        match mode {
            SemFilterMode::LowPass => gpioc_idr |= 1 << 4,
            SemFilterMode::BandPass => gpioc_idr |= 1 << 9,
            SemFilterMode::HighPass | SemFilterMode::Notch => gpioc_idr |= 1 << 12,
        }
        self.uc.get_data_mut().mmio.gpio.ports[2].idr = gpioc_idr;
        let _ = self.uc.mem_write(0x4002_0810, &gpioc_idr.to_le_bytes());
    }

    pub fn get_panel_leds(&self) -> [u8; 35] {
        let mut leds = self.coproc.get_panel_leds();
        let gpioc_odr = self.uc.get_data().mmio.gpio.ports[2].odr;
        leds[7] = if (gpioc_odr & (1 << 8)) != 0 { 255 } else { 0 };  // PC8 = LPF LED
        leds[8] = if (gpioc_odr & (1 << 10)) != 0 { 255 } else { 0 }; // PC10 = BPF LED
        leds[9] = if (gpioc_odr & (1 << 13)) != 0 { 255 } else { 0 }; // PC13 = HPF LED
        leds
    }

    pub fn get_icon_leds(&self) -> [u8; 8] {
        self.coproc.get_icon_leds()
    }

    /// Read physical rear-panel CV, Gate, and Clock I/O state
    pub fn get_rear_panel(&self) -> RearPanelState {
        let mmio = &self.uc.get_data().mmio;

        // 1. Pitch CV: TIM3 CCR1 (fine 8-bit LSB, PC6) and CCR2 (coarse 8-bit MSB, PC7)
        // TimerManager::timers[2] is TIM3
        let tim3 = &mmio.timers.timers[2];
        let ccr1 = (tim3.ccr1 & 0xFF) as u16;
        let ccr2 = (tim3.ccr2 & 0xFF) as u16;
        let pitch_cv_raw = (ccr2 << 8) | ccr1;
        let pitch_cv_volts = (pitch_cv_raw as f32 / 65535.0) * 10.0;

        // 2. Gate Output: GPIOE PE1 (contact state) and PE13 (circuit rail select: 12V vs 5V)
        // GpioPeripheral::ports[4] is GPIOE
        let gpioe_odr = mmio.gpio.ports[4].odr;
        let gate_raw = (gpioe_odr & (1 << 1)) != 0;
        let gate_mode_12v = (gpioe_odr & (1 << 13)) != 0;
        let gate_volts = if gate_raw {
            if gate_mode_12v { 12.0 } else { 5.0 }
        } else {
            0.0
        };

        // 3. Pressure CV: DAC1 Channel 1 (PA4 -> dor1, 12-bit)
        let pressure_raw = (mmio.dac.dor1 & 0x0FFF) as u16;
        let pressure_volts = (pressure_raw as f32 / 4095.0) * 10.0;

        // 4. Clock Output: GPIOE PE12
        let clock_out_raw = (gpioe_odr & (1 << 12)) != 0;
        let clock_out_volts = if clock_out_raw { 5.0 } else { 0.0 };

        // 5. Clock Input: GPIOE PE14/PE15
        let gpioe_idr = mmio.gpio.ports[4].idr;
        let clock_in_raw = (gpioe_idr & ((1 << 14) | (1 << 15))) != 0;

        let codec = &mmio.i2c1.codec;

        RearPanelState {
            pitch_cv_raw,
            pitch_cv_volts,
            gate_raw,
            gate_mode_12v,
            gate_volts,
            pressure_raw,
            pressure_volts,
            clock_out_raw,
            clock_out_volts,
            clock_in_raw,
            headset_jack: codec.get_headset_jack(),
            headset_button: codec.get_headset_button(),
            audio_input_source: codec.get_audio_input_source(),
            adc_gain_left_db: codec.get_left_adc_gain_db(),
            adc_gain_right_db: codec.get_right_adc_gain_db(),
            dac_volume_left_db: codec.get_left_dac_volume_db(),
            dac_volume_right_db: codec.get_right_dac_volume_db(),
        }
    }

    /// Sets the physical connection status of the rear-panel 3.5mm headset jack.
    pub fn set_headset_jack(&mut self, state: HeadsetJackState) {
        self.uc.get_data_mut().mmio.i2c1.codec.set_headset_jack(state);
    }

    /// Gets the current physical connection status of the rear-panel headset jack.
    pub fn get_headset_jack(&self) -> HeadsetJackState {
        self.uc.get_data().mmio.i2c1.codec.get_headset_jack()
    }

    /// Sets the physical headset inline button / hook-switch contact state.
    pub fn set_headset_button(&mut self, pressed: bool) {
        self.uc.get_data_mut().mmio.i2c1.codec.set_headset_button(pressed);
    }

    /// Gets the physical headset inline button contact state.
    pub fn get_headset_button(&self) -> bool {
        self.uc.get_data().mmio.i2c1.codec.get_headset_button()
    }

    /// Configures the selected physical audio input channel.
    pub fn set_audio_input_source(&mut self, source: AudioInputSource) {
        self.uc.get_data_mut().mmio.i2c1.codec.set_audio_input_source(source);
    }

    /// Gets the selected physical audio input channel.
    pub fn get_audio_input_source(&self) -> AudioInputSource {
        self.uc.get_data().mmio.i2c1.codec.get_audio_input_source()
    }

    /// Feeds raw audio PCM samples (-1.0 to +1.0) into the codec ADC input FIFO.
    pub fn feed_audio_input(&mut self, samples: &[f32]) {
        let src = self.uc.get_data().mmio.i2c1.codec.get_audio_input_source();
        self.uc.get_data_mut().mmio.i2c1.codec.feed_audio_input(src, samples);
        self.audio_input_countdown = 32;
    }

    /// Clears the pending audio input buffer in the codec.
    pub fn clear_audio_input(&mut self) {
        self.uc.get_data_mut().mmio.i2c1.codec.clear_input_buffer();
        self.audio_input_countdown = 0;
    }

    /// Returns a reference to the TLV320AIC3104 codec model.
    pub fn get_codec(&self) -> &crate::peripherals::tlv320aic3104::Tlv320aic3104 {
        &self.uc.get_data().mmio.i2c1.codec
    }

    /// Returns a mutable reference to the TLV320AIC3104 codec model.
    pub fn get_codec_mut(&mut self) -> &mut crate::peripherals::tlv320aic3104::Tlv320aic3104 {
        &mut self.uc.get_data_mut().mmio.i2c1.codec
    }

    /// Returns a reference to the DMA2 controller model.
    pub fn get_dma2(&self) -> &crate::peripherals::dma::DmaController {
        &self.uc.get_data().mmio.dma2
    }

    /// Drive physical Clock In electrical state (routed to GPIOE PE14 & PE15 and EXTI lines 14 & 15)
    pub fn set_clock_in(&mut self, high: bool) {
        let mask = (1 << 14) | (1 << 15);
        let new_idr = {
            let data = self.uc.get_data_mut();
            let old_idr = data.mmio.gpio.ports[4].idr;
            if high {
                data.mmio.gpio.ports[4].idr |= mask;
            } else {
                data.mmio.gpio.ports[4].idr &= !mask;
            }
            let new_idr = data.mmio.gpio.ports[4].idr;
            let rising = (new_idr & !old_idr) & mask;
            let falling = (!new_idr & old_idr) & mask;
            let exti = &mut data.mmio.exti;
            let mut triggered = (rising & exti.rtsr) | (falling & exti.ftsr);

            // If neither edge mask is specifically configured yet, trigger on rising edge
            if triggered == 0 && high && (old_idr & mask) == 0 {
                triggered = mask;
            }

            if triggered != 0 {
                exti.pr |= triggered;
                data.nvic.enable_vector(VECTOR_EXTI15_10);
                data.nvic.set_pending(VECTOR_EXTI15_10);
            }
            new_idr
        };

        let _ = self.uc.mem_write(0x4002_1010, &new_idr.to_le_bytes());
    }

    pub fn step_preset_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("preset", delta);
    }

    pub fn step_matrix_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("matrix", delta);
    }

    pub fn step_type_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("type", delta);
    }

    pub fn step_rate_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("rate", delta);
    }

    pub fn step_lfo_rate_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("lfo_rate", delta);
    }

    pub fn step_wave_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("wave", delta);
    }

    pub fn step_timbre_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("timbre", delta);
    }

    pub fn step_shape_encoder(&mut self, delta: i16) {
        self.activity_countdown = 150;
        self.coproc.step_encoder("shape", delta);
    }

    pub fn set_potentiometer(&mut self, cid: u16, value_12b: u16) {
        self.activity_countdown = 150;
        let group = (cid >> 8) as u8;
        let id = (cid & 0xFF) as u8;
        self.coproc.set_potentiometer(group, id, value_12b);
    }

    pub fn set_key(&mut self, key_idx: usize, pressure: u8) {
        self.activity_countdown = self.activity_countdown.max(60);
        self.coproc.set_key(key_idx, pressure);
    }

    pub fn set_touch_strip(&mut self, pos_12b: u16, touched: bool) {
        self.activity_countdown = self.activity_countdown.max(60);
        self.coproc.set_touch_strip(pos_12b, touched);
    }

    pub fn execute_swd_command(&mut self, line: &str) -> String {
        crate::swd::dispatch_swd_command(self, line)
    }

    pub fn is_halted(&self) -> bool {
        self.swd.is_halted()
    }

    pub fn set_halted(&mut self, halted: bool) {
        self.swd.set_halted(halted);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unmapped_memory_on_demand_mapping() {
        let mut uc = Unicorn::new_with_data(
            Arch::ARM,
            Mode::THUMB | Mode::MCLASS,
            EmulatorContext {
                mmio: MmioBus::new(),
                nvic: NvicController::default(),
                tx_bytes: Vec::new(),
                usart1_tx_bytes: Vec::new(),
                boot_splash_finished: false,
                recent_pcs: std::collections::VecDeque::new(),
                mmio_read_count: 0,
                mmio_write_count: 0,
            },
        )
        .unwrap();

        uc.add_mem_hook(
            HookType::MEM_READ_UNMAPPED,
            0,
            0xFFFF_FFFF,
            |uc, _access, addr, _size, _val| {
                let page_addr = addr & !0xFFF;
                if uc.mem_map(page_addr, 0x1000, Prot::ALL).is_ok() {
                    let zeros = [0u8; 4096];
                    let _ = uc.mem_write(page_addr, &zeros);
                    true
                } else {
                    false
                }
            },
        )
        .unwrap();

        // Map code page at FLASH_BASE (0x0800_0000)
        uc.mem_map(FLASH_BASE, 0x1000, Prot::ALL).unwrap();

        // Code: LDR R0, [R1] (0x6808); NOP (0xBF00)
        let code: [u16; 2] = [0x6808, 0xBF00];
        let mut code_bytes = [0u8; 4];
        code_bytes[0..2].copy_from_slice(&code[0].to_le_bytes());
        code_bytes[2..4].copy_from_slice(&code[1].to_le_bytes());
        uc.mem_write(FLASH_BASE, &code_bytes).unwrap();

        let unmapped_addr = 0x6000_1234u64;
        uc.reg_write(RegisterARM::R1, unmapped_addr).unwrap();
        uc.reg_write(RegisterARM::PC, (FLASH_BASE | 1) as u64).unwrap();

        // Emulate 1 instruction (LDR R0, [R1])
        let res = uc.emu_start((FLASH_BASE | 1) as u64, (FLASH_BASE + 2) as u64, 0, 1);
        assert!(res.is_ok());

        let r0 = uc.reg_read(RegisterARM::R0).unwrap();
        assert_eq!(r0, 0);
    }

    #[test]
    fn test_quadspi_unaligned_mmio() {
        let mut uc = Unicorn::new_with_data(
            Arch::ARM,
            Mode::THUMB | Mode::MCLASS,
            EmulatorContext {
                mmio: MmioBus::new(),
                nvic: NvicController::default(),
                tx_bytes: Vec::new(),
                usart1_tx_bytes: Vec::new(),
                boot_splash_finished: false,
                recent_pcs: std::collections::VecDeque::new(),
                mmio_read_count: 0,
                mmio_write_count: 0,
            },
        )
        .unwrap();

        uc.mem_map(FLASH_BASE, 0x1000, Prot::ALL).unwrap();
        uc.mem_map(QUADSPI_REG_BASE, QUADSPI_REG_SIZE as u64, Prot::ALL).unwrap();

        uc.add_mem_hook(
            HookType::MEM_READ,
            0xA000_0000,
            0xA001_0000 - 1,
            |uc, _access, addr, size, _val| {
                let aligned_addr = (addr as u32) & !3;
                let shift = ((addr as u32) & 3) * 8;
                let reg_val = uc.get_data_mut().mmio.read(aligned_addr, 4);
                let val = (reg_val >> shift) as u32;
                let _ = uc.mem_write(addr, &val.to_le_bytes()[..size.min(4)]);
                true
            },
        )
        .unwrap();

        uc.add_mem_hook(
            HookType::MEM_WRITE,
            0xA000_0000,
            0xA001_0000 - 1,
            |uc, _access, addr, size, val| {
                let addr_u32 = addr as u32;
                let aligned_addr = addr_u32 & !3;
                let shift = (addr_u32 & 3) * 8;
                let mask = match size {
                    1 => 0xFFu32,
                    2 => 0xFFFFu32,
                    _ => 0xFFFF_FFFFu32,
                };
                let raw_val = (val as u32) & mask;
                let full_val = if size < 4 {
                    let cur = uc.get_data_mut().mmio.read(aligned_addr, 4);
                    (cur & !(mask << shift)) | (raw_val << shift)
                } else {
                    raw_val
                };
                uc.get_data_mut().mmio.write(aligned_addr, 4, full_val);
                true
            },
        )
        .unwrap();

        // Code: STRB R0, [R1] (0x7008); LDRB R2, [R1] (0x780A); NOP (0xBF00)
        let code: [u16; 3] = [0x7008, 0x780A, 0xBF00];
        let mut code_bytes = [0u8; 6];
        for (i, &w) in code.iter().enumerate() {
            code_bytes[i * 2..i * 2 + 2].copy_from_slice(&w.to_le_bytes());
        }
        uc.mem_write(FLASH_BASE, &code_bytes).unwrap();

        uc.reg_write(RegisterARM::R0, 0x42).unwrap();
        uc.reg_write(RegisterARM::R1, (QUADSPI_CR + 1) as u64).unwrap(); // Target CR register byte 1
        uc.reg_write(RegisterARM::PC, (FLASH_BASE | 1) as u64).unwrap();

        let res = uc.emu_start((FLASH_BASE | 1) as u64, (FLASH_BASE + 4) as u64, 0, 2);
        assert!(res.is_ok());

        let cr_val = uc.get_data_mut().mmio.read(QUADSPI_CR, 4);
        assert_eq!((cr_val >> 8) & 0xFF, 0x42);

        let r2 = uc.reg_read(RegisterARM::R2).unwrap();
        assert_eq!(r2, 0x42);
    }

    static TEST_BIN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    fn make_test_coproc_bins(temp_dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let id = TEST_BIN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let pid = std::process::id();
        let oled_path = temp_dir.join(format!("test_dummy_oled_{}_{}.bin", pid, id));
        let matrix_path = temp_dir.join(format!("test_dummy_matrix_{}_{}.bin", pid, id));
        let kbd_path = temp_dir.join(format!("test_dummy_kbd_{}_{}.bin", pid, id));

        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let oled_src = repo_root.join("original_firmware/unpacked/nanowave_iochip_oled_payload_stm32f0.bin");
        let matrix_src = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
        let kbd_src = repo_root.join("original_firmware/unpacked/nanowave_iochip_kbd_payload_stm32f0.bin");

        if oled_src.exists() && matrix_src.exists() && kbd_src.exists() {
            let _ = std::fs::copy(&oled_src, &oled_path);
            let _ = std::fs::copy(&matrix_src, &matrix_path);
            let _ = std::fs::copy(&kbd_src, &kbd_path);
            return (oled_path, matrix_path, kbd_path);
        }

        let mut dummy = vec![0u8; 1024];
        dummy[0..4].copy_from_slice(&0x2000_1000u32.to_le_bytes());
        dummy[4..8].copy_from_slice(&0x0800_0009u32.to_le_bytes());
        dummy[8..10].copy_from_slice(&[0xFE, 0xE7]);

        let _ = std::fs::write(&oled_path, &dummy);
        let _ = std::fs::write(&matrix_path, &dummy);
        let _ = std::fs::write(&kbd_path, &dummy);

        (oled_path, matrix_path, kbd_path)
    }

    #[test]
    fn test_dma2_stream4_dbm_handling() {
        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes()); // Initial SP
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes()); // Reset handler

        // VECTOR_DMA2_STREAM4 (vector 76 at offset 76 * 4 = 304)
        let isr_addr = STOCK_FW_LOAD_ADDR + 0x200;
        dummy_bin[VECTOR_DMA2_STREAM4 as usize * 4..VECTOR_DMA2_STREAM4 as usize * 4 + 4]
            .copy_from_slice(&(isr_addr | 1).to_le_bytes());
        // Put BX LR (0x4770) at offset 0x200
        dummy_bin[0x200..0x202].copy_from_slice(&0x4770u16.to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let dummy_path = temp_dir.join("test_dummy_fw.bin");
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);
        emu.is_booted = true;

        let ping_addr = 0x2000_1000u32;
        let pong_addr = 0x2000_1100u32;

        let ping_pattern = [0x55u8; AUDIO_BLOCK_BYTES];
        let pong_pattern = [0xAAu8; AUDIO_BLOCK_BYTES];

        emu.uc.mem_write(ping_addr as u64, &ping_pattern).unwrap();
        emu.uc.mem_write(pong_addr as u64, &pong_pattern).unwrap();

        // Configure DMA2 Stream 4:
        // EN = 1 (bit 0), DBM = 1 (bit 18), CT = 0 (bit 19)
        emu.uc.get_data_mut().mmio.dma2.streams[4].cr = (1 << 0) | (1 << 18);
        emu.uc.get_data_mut().mmio.dma2.streams[4].m0ar = ping_addr;
        emu.uc.get_data_mut().mmio.dma2.streams[4].m1ar = pong_addr;

        let block1 = emu.render_raw_audio_block();
        assert_eq!(block1, ping_pattern);
        let hisr1 = emu.uc.get_data().mmio.dma2.hisr;
        assert_ne!(hisr1 & (1 << 5), 0, "TCIF4 must be asserted in HISR");
        let cr1 = emu.uc.get_data().mmio.dma2.streams[4].cr;
        assert_ne!(cr1 & (1 << 19), 0, "CT must be toggled to 1");

        // Clear TCIF4 (as the ISR would do via HIFCR)
        emu.uc.get_data_mut().mmio.dma2.hisr &= !(1 << 5);

        let block2 = emu.render_raw_audio_block();
        assert_eq!(block2, pong_pattern);
        let hisr2 = emu.uc.get_data().mmio.dma2.hisr;
        assert_ne!(hisr2 & (1 << 5), 0, "TCIF4 must be asserted in HISR");
        let cr2 = emu.uc.get_data().mmio.dma2.streams[4].cr;
        assert_eq!(cr2 & (1 << 19), 0, "CT must be toggled back to 0");
    }

    #[test]
    fn test_microswitch_physical_contact_frames() {
        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let dummy_path = temp_dir.join("test_dummy_btn_fw.bin");
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // Press Paraphonic microswitch (contact closed)
        emu.set_microswitch("paraphonic", true);
        emu.flush_coproc_to_usart();
        let mut press_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            press_bytes.push(b);
        }
        let press_frame = press_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix press frame not found on USART2 bus");
        assert_eq!(press_frame[2], 0x01, "cmd must be 0x01 (press)");
        assert_eq!(press_frame[3], 0x07, "group must be 0x07 (Paraphonic group)");
        assert_eq!(press_frame[4], 0x00, "id must be 0x00 (Paraphonic ID)");
        assert_eq!(press_frame[6], 0x0C, "state descriptor byte must be 0x0C for Contact 31");

        // Release Paraphonic microswitch (contact open)
        emu.set_microswitch("paraphonic", false);
        emu.flush_coproc_to_usart();
        let mut release_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            release_bytes.push(b);
        }
        let release_frame = release_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix release frame not found on USART2 bus");
        assert_eq!(release_frame[2], 0x02, "cmd must be 0x02 (release)");
        assert_eq!(release_frame[3], 0x07, "group must be 0x07");
        assert_eq!(release_frame[4], 0x00, "id must be 0x00");
        assert_eq!(release_frame[6], 0x0C, "state descriptor byte must be 0x0C for Contact 31");

        // Press Amp Mod microswitch (contact closed)
        emu.set_microswitch("amp_mod", true);
        emu.flush_coproc_to_usart();
        let mut amp_press_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            amp_press_bytes.push(b);
        }
        let amp_press_frame = amp_press_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix Amp Mod press frame not found on USART2 bus");
        assert_eq!(amp_press_frame[2], 0x01, "cmd must be 0x01 (press)");
        assert_eq!(amp_press_frame[3], 0x06, "group must be 0x06 (Amp Mod group)");
        assert_eq!(amp_press_frame[4], 0x00, "id must be 0x00 (Amp Mod ID)");
        assert_eq!(amp_press_frame[6], 0x01, "state must be 0x01 (contact closed)");

        // Release Amp Mod microswitch (contact open)
        emu.set_microswitch("amp_mod", false);
        emu.flush_coproc_to_usart();
        let mut amp_release_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            amp_release_bytes.push(b);
        }
        let amp_release_frame = amp_release_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix Amp Mod release frame not found on USART2 bus");
        assert_eq!(amp_release_frame[2], 0x02, "cmd must be 0x02 (release)");
        assert_eq!(amp_release_frame[3], 0x06, "group must be 0x06");
        assert_eq!(amp_release_frame[4], 0x00, "id must be 0x00");
        assert_eq!(amp_release_frame[6], 0x01, "state must be 0x01");
    }

    #[test]
    fn test_encoder_with_microswitch_physical_contact_frames() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let matrix_src = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
        if !matrix_src.exists() {
            eprintln!("Skipping test: matrix firmware binary not found");
            return;
        }

        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let dummy_path = temp_dir.join("test_dummy_enc_btn_fw.bin");
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // Preset encoder microswitch: press (contact closed)
        emu.set_microswitch("preset", true);
        emu.flush_coproc_to_usart();
        let mut preset_press_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            preset_press_bytes.push(b);
        }
        let preset_press = preset_press_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix press frame not found on USART2 bus");
        assert_eq!(preset_press[2], 0x01, "cmd must be 0x01 (press)");
        assert_eq!(preset_press[3], 0x08, "group must be 0x08 (Preset button group)");
        assert_eq!(preset_press[4], 0x01, "id must be 0x01");

        // Preset encoder microswitch: release (contact open)
        emu.set_microswitch("preset", false);
        emu.flush_coproc_to_usart();
        let mut preset_release_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            preset_release_bytes.push(b);
        }
        let preset_release = preset_release_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix release frame not found on USART2 bus");
        assert_eq!(preset_release[2], 0x02, "cmd must be 0x02 (release)");
        assert_eq!(preset_release[3], 0x08, "group must be 0x08");
        assert_eq!(preset_release[4], 0x01, "id must be 0x01");

        // Preset encoder rotation: delta = +1
        emu.step_preset_encoder(1);
        emu.flush_coproc_to_usart();
        let mut preset_rot_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            preset_rot_bytes.push(b);
        }
        let preset_rot = preset_rot_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[2] == 0x0F)
            .expect("Matrix encoder rotation frame not found on USART2 bus");
        assert_eq!(preset_rot, [0xAA, 0x00, 0x0F, 0x08, 0x00, 0x00, 0x01, 0xC2]);

        // Rate encoder microswitch: press (contact closed, PC5, Contact 13)
        emu.set_microswitch("rate", true);
        emu.flush_coproc_to_usart();
        let mut rate_press_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            rate_press_bytes.push(b);
        }
        let rate_press = rate_press_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix rate press frame not found on USART2 bus");
        assert_eq!(rate_press[2], 0x01, "cmd must be 0x01 (press)");
        assert_eq!(rate_press[3], 0x04, "group must be 0x04 (Arp/Seq group)");
        assert_eq!(rate_press[4], 0x05, "id must be 0x05 (Rate push ID)");

        // Rate encoder microswitch: release (contact open, PC5, Contact 13)
        emu.set_microswitch("rate", false);
        emu.flush_coproc_to_usart();
        let mut rate_release_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            rate_release_bytes.push(b);
        }
        let rate_release = rate_release_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix rate release frame not found on USART2 bus");
        assert_eq!(rate_release[2], 0x02, "cmd must be 0x02 (release)");
        assert_eq!(rate_release[3], 0x04, "group must be 0x04");
        assert_eq!(rate_release[4], 0x05, "id must be 0x05");

        // Oct | Mod microswitch: press (contact closed, PC4, Contact 12)
        emu.set_microswitch("oct_mod", true);
        emu.flush_coproc_to_usart();
        let mut oct_mod_press_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            oct_mod_press_bytes.push(b);
        }
        let oct_mod_press = oct_mod_press_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix oct_mod press frame not found on USART2 bus");
        assert_eq!(oct_mod_press[2], 0x01, "cmd must be 0x01 (press)");
        assert_eq!(oct_mod_press[3], 0x04, "group must be 0x04 (Arp/Seq group)");
        assert_eq!(oct_mod_press[4], 0x01, "id must be 0x01 (Oct | Mod ID)");

        // Oct | Mod microswitch: release (contact open, PC4, Contact 12)
        emu.set_microswitch("oct_mod", false);
        emu.flush_coproc_to_usart();
        let mut oct_mod_release_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            oct_mod_release_bytes.push(b);
        }
        let oct_mod_release = oct_mod_release_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[1] == 0x20)
            .expect("Matrix oct_mod release frame not found on USART2 bus");
        assert_eq!(oct_mod_release[2], 0x02, "cmd must be 0x02 (release)");
        assert_eq!(oct_mod_release[3], 0x04, "group must be 0x04");
        assert_eq!(oct_mod_release[4], 0x01, "id must be 0x01");
    }

    #[test]
    fn test_potentiometer_coproc_frames() {
        let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let matrix_src = repo_root.join("original_firmware/unpacked/nanowave_iochip_matrix_payload_stm32f0.bin");
        if !matrix_src.exists() {
            eprintln!("Skipping test: matrix firmware binary not found");
            return;
        }

        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let dummy_path = temp_dir.join("test_dummy_pot_fw.bin");
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // Glide potentiometer: CID 0x0300, 12-bit value 3500 (0x0DAC)
        emu.set_potentiometer(0x0300, 3500);
        emu.flush_coproc_to_usart();

        let mut glide_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            glide_bytes.push(b);
        }
        let glide_frame = glide_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[2] == 0x03)
            .expect("Matrix potentiometer frame not found on USART2 bus");
        assert_eq!(glide_frame[2], 0x03, "cmd must be 0x03 (potentiometer)");
        assert_eq!(glide_frame[3], 0x03, "group must be 0x03 (Glide group)");
        assert_eq!(glide_frame[4], 0x00, "id must be 0x00 (Glide ID)");
        assert_eq!(glide_frame[5], 0x6D, "val_hi must be 0x6D");
        assert_eq!(glide_frame[6], 0x65, "val_lo must be 0x65");
        assert_eq!(glide_frame[7], 0x82, "checksum must match");

        // Cutoff potentiometer: CID 0x0101, 12-bit value 3500 (0x0DAC)
        emu.set_potentiometer(0x0101, 3500);
        emu.flush_coproc_to_usart();

        let mut cutoff_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            cutoff_bytes.push(b);
        }
        let cutoff_frame = cutoff_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[2] == 0x03 && w[3] == 0x01 && w[4] == 0x01)
            .expect("Matrix cutoff frame not found on USART2 bus");
        assert_eq!(cutoff_frame[2], 0x03, "cmd must be 0x03 (potentiometer)");
        assert_eq!(cutoff_frame[3], 0x01, "group must be 0x01 (Cutoff group)");
        assert_eq!(cutoff_frame[4], 0x01, "id must be 0x01 (Cutoff ID)");

        // Resonance potentiometer: CID 0x0102, 12-bit value 2048 (0x0800)
        emu.set_potentiometer(0x0102, 2048);
        emu.flush_coproc_to_usart();

        let mut res_bytes = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            res_bytes.push(b);
        }
        let res_frame = res_bytes
            .windows(8)
            .find(|w| w[0] == 0xAA && w[2] == 0x03 && w[3] == 0x01 && w[4] == 0x02)
            .expect("Matrix resonance frame not found on USART2 bus");
        assert_eq!(res_frame[2], 0x03, "cmd must be 0x03 (potentiometer)");
        assert_eq!(res_frame[3], 0x01, "group must be 0x01 (Resonance group)");
        assert_eq!(res_frame[4], 0x02, "id must be 0x02 (Resonance ID)");
        assert_eq!(res_frame[5], 0x40, "val_hi must be 0x40");
        assert_eq!(res_frame[6], 0x03, "val_lo must be 0x03");
        assert_eq!(res_frame[7], 0xF3, "checksum must match");
    }

    #[test]
    fn test_keyboard_physical_touch_and_pitch_mod_frames() {
        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let dummy_path = temp_dir.join("test_dummy_kbd_touch_fw.bin");
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // 1. Key press: Key 5, pressure 80
        emu.set_key(5, 80);
        emu.coproc.step_all(9600 * 110);
        emu.flush_coproc_to_usart();
        let mut key_press_frame = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            key_press_frame.push(b);
        }
        assert!(
            key_press_frame.windows(8).any(|w| w == [0xAA, 0x30, 0x07, 0x01, 0x05, 0x01, 0xFF, 0xE7]),
            "Expected Key 5 autonomous touch frame in key_press_frame: {:02X?}",
            key_press_frame
        );


        // Key release: Key 5, pressure 0
        emu.set_key(5, 0);
        emu.coproc.step_all(9600 * 110);
        emu.flush_coproc_to_usart();
        let mut key_release_frame = Vec::new();
        while let Some(b) = emu.uc.get_data_mut().mmio.usart2.rx_queue.pop_front() {
            key_release_frame.push(b);
        }
        assert!(
            key_release_frame.windows(8).any(|w| w == [0xAA, 0x30, 0x07, 0x01, 0x05, 0x00, 0x00, 0xE7]),
            "Expected Key 5 autonomous release frame in key_release_frame: {:02X?}",
            key_release_frame
        );
    }

    #[test]
    fn test_usart1_midi_ingestion_and_rxne() {
        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let dummy_path = temp_dir.join("test_dummy_midi_fw.bin");
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // Feed MIDI NoteOn message (0x90, 0x3C = Middle C, 0x7F = Vel 127)
        let midi_msg = [0x90u8, 0x3C, 0x7F];
        emu.enqueue_midi_bytes(&midi_msg);

        // Verify status register reports RXNE (bit 5)
        let sr = emu.uc.get_data().mmio.usart1.read_sr();
        assert_ne!(sr & (1 << 5), 0, "USART1_SR RXNE bit must be set when bytes in FIFO");

        // Verify reading data register returns authentic bytes in sequence
        assert_eq!(emu.uc.get_data_mut().mmio.usart1.read_dr(), 0x90);
        assert_eq!(emu.uc.get_data_mut().mmio.usart1.read_dr(), 0x3C);
        assert_eq!(emu.uc.get_data_mut().mmio.usart1.read_dr(), 0x7F);

        // FIFO is now empty; RXNE should be cleared
        let sr_empty = emu.uc.get_data().mmio.usart1.read_sr();
        assert_eq!(sr_empty & (1 << 5), 0, "USART1_SR RXNE must be 0 when FIFO empty");
    }

    #[test]
    fn test_qspi_flash_disk_persistence() {
        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let pid = std::process::id();
        let dummy_path = temp_dir.join(format!("test_dummy_qspi_fw_{}.bin", pid));
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // Initially clean
        assert!(!emu.is_qspi_dirty());

        // Perform write to QSPI via peripheral method to trigger dirty flag
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_CR, 4, 1); // Enable
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_CCR, 4, 0x06); // Write Enable (0x06)
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_CCR, 4, (1 << 10) | 0x20); // Sector Erase (0x20)
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_AR, 4, 0x0000); // Address 0 triggers erase
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_CCR, 4, 0x06); // Write Enable (0x06)
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_DLR, 4, 19); // 20 bytes
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_CCR, 4, (1 << 10) | 0x02); // Page program (0x02)
        emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_AR, 4, 0x0000); // Address 0 triggers page program
        for &b in b"MICROFREAK_PRESET_V1" {
            emu.uc.get_data_mut().mmio.qspi.write(QUADSPI_DR, 1, b as u32);
        }

        assert!(emu.is_qspi_dirty(), "QSPI write must mark peripheral dirty");

        let persist_path = temp_dir.join(format!("test_user_presets_{}.bin", pid));
        emu.flush_qspi_to_disk(&persist_path).unwrap();
        emu.clear_qspi_dirty();
        assert!(!emu.is_qspi_dirty(), "clear_qspi_dirty must reset dirty flag");
        assert!(persist_path.exists());

        // Corrupt memory in emulator
        emu.uc.get_data_mut().mmio.qspi.flash_memory[0..20].fill(0x00);

        // Reload from disk
        emu.load_qspi_from_disk(&persist_path).unwrap();
        assert_eq!(&emu.uc.get_data().mmio.qspi.flash_memory[0..20], b"MICROFREAK_PRESET_V1");

        let _ = std::fs::remove_file(&persist_path);
    }

    #[test]
    fn test_usb_otg_fs_mmio_and_midi_routing() {
        let mut dummy_bin = vec![0u8; 1024];
        dummy_bin[0..4].copy_from_slice(&0x2001_0000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&(STOCK_FW_LOAD_ADDR | 1).to_le_bytes());

        let temp_dir = std::env::temp_dir();
        let (oled_p, matrix_p, kbd_p) = make_test_coproc_bins(&temp_dir);
        let pid = std::process::id();
        let dummy_path = temp_dir.join(format!("test_dummy_usb_fw_{}.bin", pid));
        std::fs::write(&dummy_path, &dummy_bin).unwrap();
        let mut emu = VibeFreakEmulator::new(&dummy_path, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_path);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // 1. Verify firmware-facing register reads through MmioBus
        let grstctl = emu.uc.get_data_mut().mmio.read(USB_OTG_FS_BASE as u32 + 0x10, 4);
        assert_eq!(grstctl & (1 << 5), 0, "GRSTCTL TXFFLSH must be 0");

        let diepint1 = emu.uc.get_data_mut().mmio.read(USB_OTG_FS_BASE as u32 + 0x928, 4);
        assert_ne!(diepint1 & (1 << 7), 0, "DIEPINT1 TXFE bit 7 must be set for FIFO transmit readiness");

        let dtxfsts1 = emu.uc.get_data_mut().mmio.read(USB_OTG_FS_BASE as u32 + 0x938, 4);
        assert!(dtxfsts1 > 0x7F, "DTXFSTS1 must report > 0x7F words available");

        // 2. Simulate firmware writing a USB-MIDI packet to FIFO 1 at 0x5000_1000
        let note_packet = [0x09u8, 0x90, 0x3C, 0x7F]; // Note On
        emu.uc.get_data_mut().mmio.write(USB_OTG_FS_BASE as u32 + 0x1000, 4, u32::from_le_bytes(note_packet));

        let tx = emu.drain_usb_midi_packets();
        assert_eq!(tx.len(), 1);
        assert_eq!(tx[0], note_packet);

        // 3. Queue USB-MIDI packet from host and read back through FIFO
        let cc_packet = [0x0Bu8, 0xB0, 0x01, 0x40]; // Mod wheel
        emu.enqueue_usb_midi_packet(cc_packet);

        let rx_fifo_val = emu.uc.get_data_mut().mmio.read(USB_OTG_FS_BASE as u32 + 0x1000, 4);
        assert_eq!(rx_fifo_val.to_le_bytes(), cc_packet);
    }

    #[test]
    fn test_vfp_fast_path_toggle() {
        let temp_dir = std::env::temp_dir();
        let pid = std::process::id();
        let oled_p = temp_dir.join(format!("test_vfp_oled_{}.bin", pid));
        let matrix_p = temp_dir.join(format!("test_vfp_matrix_{}.bin", pid));
        let kbd_p = temp_dir.join(format!("test_vfp_kbd_{}.bin", pid));
        let dummy_fw = temp_dir.join(format!("test_vfp_fw_{}.bin", pid));
        std::fs::write(&oled_p, [0u8; 1024]).unwrap();
        std::fs::write(&matrix_p, [0u8; 1024]).unwrap();
        std::fs::write(&kbd_p, [0u8; 1024]).unwrap();

        let mut dummy_bin = vec![0u8; 0x1000];
        dummy_bin[0..4].copy_from_slice(&0x2000_1000u32.to_le_bytes());
        dummy_bin[4..8].copy_from_slice(&0x0800_0009u32.to_le_bytes());
        std::fs::write(&dummy_fw, &dummy_bin).unwrap();

        let mut emu = VibeFreakEmulator::new(&dummy_fw, None::<&Path>, &oled_p, &matrix_p, &kbd_p).unwrap();
        let _ = std::fs::remove_file(&dummy_fw);
        let _ = std::fs::remove_file(&oled_p);
        let _ = std::fs::remove_file(&matrix_p);
        let _ = std::fs::remove_file(&kbd_p);

        // Verify default is enabled
        assert!(emu.is_vfp_fast_path_enabled(), "VFP Fast-Path must be enabled by default");

        // Verify toggle off
        emu.set_vfp_fast_path(false);
        assert!(!emu.is_vfp_fast_path_enabled(), "VFP Fast-Path must be disabled after set_vfp_fast_path(false)");

        // Verify toggle back on
        emu.set_vfp_fast_path(true);
        assert!(emu.is_vfp_fast_path_enabled(), "VFP Fast-Path must be enabled after set_vfp_fast_path(true)");
    }
}

