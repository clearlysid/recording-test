use anyhow::Error;
use crabgrab::frame::VideoFrame;

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

pub trait Encoder {
    fn append_frame(&mut self, video_frame: VideoFrame) -> Result<(), Error>;

    fn finish(&mut self) -> Result<(), Error>;
}