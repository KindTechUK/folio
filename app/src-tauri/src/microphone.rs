//! What Folio can tell about the default microphone's own sound processing
//! (docs/research/room-mic-capture-2026-11.md, recommendation 3).
//!
//! Detection only: how Folio captures audio doesn't change. On Windows, an
//! endpoint can offer "raw" capture that skips the driver's effects (noise
//! suppression, gain control, beamforming toward one talker), which can remove
//! distant voices in a meeting room. Whether to ask for it is decided after
//! testing on real laptops. On a Mac, the built-in microphone is always
//! processed by macOS.

use cpal::traits::{DeviceTrait, HostTrait};
use serde::Serialize;

/// Whether the system processes the microphone's sound before Folio gets it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Processing {
    /// Windows can give Folio this microphone's sound without its effects.
    #[cfg_attr(not(windows), allow(dead_code))]
    Bypassable,
    /// Windows can't give Folio this microphone's sound without its effects
    /// (the endpoint offers no raw mode). Whether any effects are on is
    /// unknown.
    #[cfg_attr(not(windows), allow(dead_code))]
    Always,
    /// A Mac's built-in microphone, which macOS always processes.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    BuiltInMac,
    /// Couldn't tell.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Microphone {
    pub name: Option<String>,
    pub processing: Processing,
}

/// Look at the default input device. Never fails: what can't be read is
/// `None` or [`Processing::Unknown`].
pub fn default_microphone() -> Microphone {
    let Some(device) = cpal::default_host().default_input_device() else {
        return Microphone { name: None, processing: Processing::Unknown };
    };
    let description = device.description().ok();
    Microphone {
        name: description.as_ref().map(|d| d.name().to_string()),
        processing: processing(&device, description.as_ref()),
    }
}

#[cfg(windows)]
fn processing(device: &cpal::Device, _: Option<&cpal::DeviceDescription>) -> Processing {
    match windows_raw::raw_supported(device) {
        Some(true) => Processing::Bypassable,
        Some(false) => Processing::Always,
        None => Processing::Unknown,
    }
}

/// cpal doesn't say whether a Mac device is built in, so ask Core Audio for
/// the default input device's transport type.
#[cfg(target_os = "macos")]
fn processing(_: &cpal::Device, _: Option<&cpal::DeviceDescription>) -> Processing {
    use objc2_core_audio::{
        kAudioDevicePropertyTransportType, kAudioDeviceTransportTypeBuiltIn,
        kAudioHardwarePropertyDefaultInputDevice, kAudioObjectPropertyElementMain,
        kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject, AudioObjectGetPropertyData,
        AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertySelector,
    };
    use std::ptr::{null, NonNull};

    fn read_u32(object: AudioObjectID, selector: AudioObjectPropertySelector) -> Option<u32> {
        let mut address = AudioObjectPropertyAddress {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: both properties are a single UInt32 (an AudioObjectID and a
        // transport type); the status is checked before the value is used.
        let status = unsafe {
            AudioObjectGetPropertyData(
                object,
                NonNull::from(&mut address),
                0,
                null(),
                NonNull::from(&mut size),
                NonNull::from(&mut value).cast(),
            )
        };
        (status == 0).then_some(value)
    }

    let transport = read_u32(kAudioObjectSystemObject as AudioObjectID, kAudioHardwarePropertyDefaultInputDevice)
        .and_then(|device| read_u32(device, kAudioDevicePropertyTransportType));
    match transport {
        Some(t) if t == kAudioDeviceTransportTypeBuiltIn => Processing::BuiltInMac,
        _ => Processing::Unknown,
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn processing(_: &cpal::Device, _: Option<&cpal::DeviceDescription>) -> Processing {
    Processing::Unknown
}

#[cfg(windows)]
mod windows_raw {
    use windows::core::GUID;
    use windows::Win32::Foundation::PROPERTYKEY;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED, STGM_READ};
    use windows::Win32::System::Variant::VT_BOOL;

    /// `PKEY_Devices_AudioDevice_RawProcessingSupported`.
    const RAW_PROCESSING_SUPPORTED: PROPERTYKEY = PROPERTYKEY {
        fmtid: GUID::from_u128(0x8943b373_388c_4395_b557_bc6dbaffafdb),
        pid: 2,
    };

    /// The endpoint's `RawProcessingSupported` property; `None` if it can't
    /// be read.
    pub(super) fn raw_supported(device: &cpal::Device) -> Option<bool> {
        // cpal initialises COM on its own threads; this one may be new.
        // Already-initialised (S_FALSE) and a different apartment
        // (RPC_E_CHANGED_MODE) both leave COM usable.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        #[allow(unreachable_patterns)]
        let immdevice = match device.as_inner() {
            cpal::platform::DeviceInner::Wasapi(d) => d.immdevice()?,
            _ => return None,
        };
        unsafe {
            let store = immdevice.OpenPropertyStore(STGM_READ).ok()?;
            let value = store.GetValue(&RAW_PROCESSING_SUPPORTED).ok()?;
            let inner = &value.Anonymous.Anonymous;
            (inner.vt == VT_BOOL).then(|| inner.Anonymous.boolVal.as_bool())
        }
    }
}
