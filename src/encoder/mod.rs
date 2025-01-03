// pub mod acffmpeg;
pub mod windowsrs;

use anyhow::Error;
use crabgrab::frame::VideoFrame;

pub trait Encoder {
    fn append_frame(&mut self, video_frame: VideoFrame) -> Result<(), Error>;

    fn finish(&mut self) -> Result<(), Error>;
}