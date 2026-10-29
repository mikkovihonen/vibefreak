pub mod audio;
pub mod config;
pub mod coproc;
pub mod core;
pub mod factory_data;
pub mod mff;
pub mod nvic;
pub mod peripherals;
pub mod profile;
pub mod svd_diagnostics;
pub mod swd;

pub use crate::audio::AudioProcessor;
pub use crate::config::*;
pub use crate::core::VibeFreakEmulator;
pub use crate::factory_data::*;
pub use crate::mff::{MffError, MffFirmwarePackage};
pub use crate::peripherals::tlv320aic3104::{AudioInputSource, HeadsetJackState, Tlv320aic3104};
pub use crate::profile::FirmwareProfile;


