
mod encoder;

use anyhow::Error;
use crabgrab::prelude::WindowsDx11VideoFrame;
use encoder::windowsrs::{ContainerSettingsBuilder, SendDirectX, VideoSettingsBuilder};
use windows::Graphics::DirectX::Direct3D11::IDirect3DSurface;
use std::sync::mpsc;
use std::path::Path;
use encoder::Encoder;
use pollster::FutureExt;

use crabgrab::capturable_content::{CapturableContent, CapturableContentFilter};
use crabgrab::capture_stream::{CaptureConfig, CaptureStream, StreamEvent, CapturePixelFormat};

// Variables to configure the stream
// Encoder configs are in the ./encoder folder
const STREAM_PX_FMT: CapturePixelFormat = CapturePixelFormat::F420;
const SCALE_FACTOR: f64 = 2.0;
const OUTPUT_FILE: &str = "./video.mp4";

fn main() -> Result<(), Error> {

    // MARK: Configure Stream
    let content = CapturableContent::new(CapturableContentFilter::DISPLAYS).block_on()?;
    let display = content.displays().next().ok_or(Error::msg("No displays found"))?;

    let size = display.rect().scaled(SCALE_FACTOR).size; // Hardcoded to 2 (as scale factor)
    let height = size.height as u32;
    let width = size.width as u32;

    let stream_cfg = CaptureConfig::with_display(display, CapturePixelFormat::Bgra8888)
        .with_output_size(size);

    let stream_token =
    CaptureStream::test_access(false).ok_or(Error::msg("Failed to get access token"))?;

    // MARK: Configure Encoder
    let output = Path::new(OUTPUT_FILE);
    let mut encoder = encoder::windowsrs::EncoderWindowsRs::new(VideoSettingsBuilder::new(width, height), ContainerSettingsBuilder::new(), output)?;

    let (tx, rx) = mpsc::channel();
    
    let handle = std::thread::spawn(move || {
        while let Ok(Some((surface, ts))) = rx.recv() {
            encoder.send_frame((surface, ts)).expect("couldn't encode frame");
        }
        
        encoder.finish().expect("couldn't finish encoding");
    });
    // MARK: Start stream
    let mut stream = CaptureStream::new(stream_token, stream_cfg, move |result| match result {
        Ok(event) => match event {
            StreamEvent::Video(frame) => {
    
                let ts = frame.capture_time();
                let (surface, _) = frame.get_dx11_surface().expect("can't get surface");
                tx.send(Some((SendDirectX::new(surface), ts))).expect("couldn't send frame");
            },
            StreamEvent::End => tx.send(None).expect("couldn't send none"),
            _ => {}
        },
        Err(e) => println!("Error: {}", e),
    })?;


    // MARK: Record for 3 seconds, then stop
    std::thread::sleep(std::time::Duration::from_secs(3));
    stream.stop()?;

    handle.join().expect("couldn't complete encoding thread");

    println!("finished!");

    Ok(())
}
