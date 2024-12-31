mod cidre;

pub use cidre::*;

use anyhow::Error;
use crabgrab::frame::VideoFrame;
use screencapturekit::output::CMSampleBuffer;

pub trait Encoder {
    fn append_frame(&mut self, sample_buffer: CMSampleBuffer, sample_index: i32) -> Result<(), Error>;

    fn finish(&mut self) -> Result<(), Error>;
}
