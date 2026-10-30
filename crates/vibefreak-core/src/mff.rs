//! Native Arturia MicroFreak (.mff) firmware archive parser and verifier.
//!
//! A `.mff` archive is a standard uncompressed/deflated ZIP container packaging:
//! - `info.json`: Release manifest containing version metadata.
//! - 5 container `.bin` images, each prepended with a 64-byte `ArturiaBinHeader`:
//!   - `0x00`: Main MCU payload (STM32F446 Cortex-M4 FPU @ 180 MHz)
//!   - `0x01`: OLED coprocessor payload (STM32F0 Cortex-M0)
//!   - `0x02`: Matrix coprocessor payload (STM32F0 Cortex-M0)
//!   - `0x03`: Keyboard coprocessor payload (STM32F0 Cortex-M0)
//!   - `0x81`: QSPI wavetable factory flash payload (320 KB)

use std::io::{Read, Seek};
use std::path::Path;
use thiserror::Error;

pub const ARTURIA_BIN_HEADER_SIZE: usize = 64;
pub const TARGET_MAIN_MCU: u8 = 0x00;
pub const TARGET_OLED_COPROC: u8 = 0x01;
pub const TARGET_MATRIX_COPROC: u8 = 0x02;
pub const TARGET_KEYBOARD_COPROC: u8 = 0x03;
pub const TARGET_QSPI_WAVETABLES: u8 = 0x81;

#[derive(Error, Debug)]
pub enum MffError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("Container too small ({0} bytes < 64) in file '{1}'")]
    HeaderTooSmall(usize, String),
    #[error("Checksum verification failed in '{0}': expected 0x{1:02X}, computed 0x{2:02X}")]
    ChecksumMismatch(String, u8, u8),
    #[error("Trailer checksum byte mismatch in '{0}': expected 0x{1:02X}, found 0x{2:02X}")]
    TrailerMismatch(String, u8, u8),
    #[error("Missing target binary in MFF archive: target ID 0x{0:02X}")]
    MissingTarget(u8),
    #[error("Empty payload for target ID 0x{0:02X} in file '{1}'")]
    EmptyPayload(u8, String),
}

/// In-memory representation of an unpacked and verified MicroFreak firmware package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MffFirmwarePackage {
    pub main_mcu: Vec<u8>,
    pub oled_coproc: Vec<u8>,
    pub matrix_coproc: Vec<u8>,
    pub kbd_coproc: Vec<u8>,
    pub qspi_wavetable: Vec<u8>,
    pub version: Option<String>,
}

/// Computes the 8-bit longitudinal XOR checksum across a byte slice.
pub fn compute_xor_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0u8, |acc, &b| acc ^ b)
}

impl MffFirmwarePackage {
    /// Loads and parses an .mff archive directly from a file path.
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self, MffError> {
        let file = std::fs::File::open(path)?;
        Self::from_reader(file)
    }

    /// Loads and parses an .mff archive from in-memory byte slice.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MffError> {
        let cursor = std::io::Cursor::new(bytes);
        Self::from_reader(cursor)
    }

    /// Loads and parses an .mff archive from any reader implementing `Read + Seek`.
    pub fn from_reader<R: Read + Seek>(reader: R) -> Result<Self, MffError> {
        let mut archive = zip::ZipArchive::new(reader)?;

        let mut main_mcu = None;
        let mut oled_coproc = None;
        let mut matrix_coproc = None;
        let mut kbd_coproc = None;
        let mut qspi_wavetable = None;
        let mut version = None;

        // Try extracting version from info.json
        if let Ok(mut info_file) = archive.by_name("info.json") {
            let mut info_str = String::new();
            if info_file.read_to_string(&mut info_str).is_ok() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&info_str) {
                    version = val.get("version_number").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
            }
        }

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let file_name = file.name()?.to_string();
            if !file_name.ends_with(".bin") {
                continue;
            }

            let mut container_bytes = Vec::with_capacity(file.size() as usize);
            file.read_to_end(&mut container_bytes)?;

            if container_bytes.len() < ARTURIA_BIN_HEADER_SIZE {
                return Err(MffError::HeaderTooSmall(container_bytes.len(), file_name));
            }

            let target_id = container_bytes[0];
            let header_csum = container_bytes[1];
            let payload = &container_bytes[ARTURIA_BIN_HEADER_SIZE..];

            if payload.is_empty() {
                return Err(MffError::EmptyPayload(target_id, file_name));
            }

            let is_qspi = (target_id & 0x80) != 0;

            if is_qspi {
                let computed = compute_xor_checksum(payload);
                if computed != header_csum {
                    return Err(MffError::ChecksumMismatch(file_name, header_csum, computed));
                }
            } else {
                let computed = compute_xor_checksum(&payload[..payload.len() - 1]);
                let trailer_byte = payload[payload.len() - 1];
                if computed != header_csum {
                    return Err(MffError::ChecksumMismatch(file_name, header_csum, computed));
                }
                if trailer_byte != header_csum {
                    return Err(MffError::TrailerMismatch(file_name, header_csum, trailer_byte));
                }
            }

            match target_id {
                TARGET_MAIN_MCU => {
                    // Check if stripped of 16-byte footer (15 zero bytes + 1 trailer byte)
                    let raw_bytes = if payload.len() >= 16
                        && payload[payload.len() - 16..payload.len() - 1] == [0u8; 15]
                    {
                        &payload[..payload.len() - 16]
                    } else {
                        payload
                    };
                    main_mcu = Some(raw_bytes.to_vec());
                }
                TARGET_OLED_COPROC => {
                    oled_coproc = Some(payload.to_vec());
                }
                TARGET_MATRIX_COPROC => {
                    matrix_coproc = Some(payload.to_vec());
                }
                TARGET_KEYBOARD_COPROC => {
                    kbd_coproc = Some(payload.to_vec());
                }
                TARGET_QSPI_WAVETABLES => {
                    qspi_wavetable = Some(payload.to_vec());
                }
                _ => {
                    log::warn!(
                        "Encountered unknown target ID 0x{:02X} in container '{}'",
                        target_id,
                        file_name
                    );
                }
            }
        }

        Ok(Self {
            main_mcu: main_mcu.ok_or(MffError::MissingTarget(TARGET_MAIN_MCU))?,
            oled_coproc: oled_coproc.ok_or(MffError::MissingTarget(TARGET_OLED_COPROC))?,
            matrix_coproc: matrix_coproc.ok_or(MffError::MissingTarget(TARGET_MATRIX_COPROC))?,
            kbd_coproc: kbd_coproc.ok_or(MffError::MissingTarget(TARGET_KEYBOARD_COPROC))?,
            qspi_wavetable: qspi_wavetable.ok_or(MffError::MissingTarget(TARGET_QSPI_WAVETABLES))?,
            version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_mock_container(target_id: u8, mut payload: Vec<u8>) -> Vec<u8> {
        let is_qspi = (target_id & 0x80) != 0;
        let csum = if is_qspi {
            compute_xor_checksum(&payload)
        } else {
            let c = compute_xor_checksum(&payload);
            payload.push(c); // append trailer byte
            c
        };

        let mut container = vec![0u8; ARTURIA_BIN_HEADER_SIZE];
        container[0] = target_id;
        container[1] = csum;
        container.extend_from_slice(&payload);
        container
    }

    fn build_mock_mff_zip(version: &str) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("info.json", options).unwrap();
            let info_json = format!(r#"{{"version_number": "{}"}}"#, version);
            zip.write_all(info_json.as_bytes()).unwrap();

            let main_payload = {
                let mut p = vec![0x11, 0x22, 0x33, 0x44, 0x55];
                p.extend_from_slice(&[0u8; 15]); // 15-byte zero padding before trailer
                p
            };

            let targets = [
                (TARGET_MAIN_MCU, "main.bin", main_payload),
                (TARGET_OLED_COPROC, "oled.bin", vec![0x66, 0x77, 0x88]),
                (TARGET_MATRIX_COPROC, "matrix.bin", vec![0x99, 0xAA, 0xBB]),
                (TARGET_KEYBOARD_COPROC, "kbd.bin", vec![0xCC, 0xDD, 0xEE]),
                (TARGET_QSPI_WAVETABLES, "wavetable.bin", vec![0x42; 327680]),
            ];

            for (target_id, filename, payload) in targets {
                zip.start_file(filename, options).unwrap();
                let container = build_mock_container(target_id, payload);
                zip.write_all(&container).unwrap();
            }

            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn test_compute_xor_checksum() {
        assert_eq!(compute_xor_checksum(&[]), 0);
        assert_eq!(compute_xor_checksum(&[0x12]), 0x12);
        assert_eq!(compute_xor_checksum(&[0x12, 0x34]), 0x12 ^ 0x34);
        assert_eq!(compute_xor_checksum(&[0xAA, 0x55, 0xAA, 0x55]), 0);
    }

    #[test]
    fn test_parse_synthetic_mock_mff() {
        let mock_zip = build_mock_mff_zip("5.0.0.9999");
        let pkg = MffFirmwarePackage::from_bytes(&mock_zip).expect("Failed to parse mock MFF");
        assert_eq!(pkg.version.as_deref(), Some("5.0.0.9999"));
        assert_eq!(pkg.main_mcu, vec![0x11, 0x22, 0x33, 0x44, 0x55]);
        assert_eq!(pkg.oled_coproc[0..3], [0x66, 0x77, 0x88]);
        assert_eq!(pkg.matrix_coproc[0..3], [0x99, 0xAA, 0xBB]);
        assert_eq!(pkg.kbd_coproc[0..3], [0xCC, 0xDD, 0xEE]);
        assert_eq!(pkg.qspi_wavetable.len(), 327680);
        assert_eq!(pkg.qspi_wavetable[0], 0x42);
    }

    #[test]
    fn test_parse_official_mff_if_present() {
        let env_mff = std::env::var("VIBEFREAK_MFF_PATH").ok().map(std::path::PathBuf::from);
        let path = env_mff.unwrap_or_else(|| {
            Path::new("../../original_firmware/microfreak/MicroFreak_Firmware_Update_5_0_0_2084.mff").to_path_buf()
        });
        if !path.exists() {
            // Also test relative to workspace root
            let root_path = Path::new("original_firmware/microfreak/MicroFreak_Firmware_Update_5_0_0_2084.mff");
            if !root_path.exists() {
                return;
            }
            let pkg = MffFirmwarePackage::from_file(root_path).expect("Failed to parse official MFF");
            assert_eq!(pkg.version.as_deref(), Some("5.0.0.2084"));
            assert_eq!(pkg.main_mcu.len(), 473680);
            assert_eq!(pkg.oled_coproc.len(), 81648);
            assert_eq!(pkg.matrix_coproc.len(), 29680);
            assert_eq!(pkg.kbd_coproc.len(), 26864);
            assert_eq!(pkg.qspi_wavetable.len(), 327680);
            return;
        }

        let pkg = MffFirmwarePackage::from_file(path).expect("Failed to parse official MFF");
        assert_eq!(pkg.version.as_deref(), Some("5.0.0.2084"));
        assert_eq!(pkg.main_mcu.len(), 473680);
        assert_eq!(pkg.oled_coproc.len(), 81648);
        assert_eq!(pkg.matrix_coproc.len(), 29680);
        assert_eq!(pkg.kbd_coproc.len(), 26864);
        assert_eq!(pkg.qspi_wavetable.len(), 327680);
    }
}
