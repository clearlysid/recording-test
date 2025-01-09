use anyhow::Error;

// mod acffmpeg;
// pub use acffmpeg::EncoderAcFfmpeg as VideoEncoder;

#[cfg(target_os = "macos")]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::AVAssetWriterEncoder as VideoEncoder;

#[cfg(target_os = "windows")]
mod win;

#[cfg(target_os = "windows")]
pub use win::WmfEncoder as VideoEncoder;

use crate::AudioSample;

pub trait Encoder {
    fn append_audio(&mut self, audio_sample: AudioSample) -> Result<(), Error>;

    fn finish(&mut self) -> Result<(), Error>;
}