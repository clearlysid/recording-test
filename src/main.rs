mod encoder;

use anyhow::Error;
use crabgrab::util::{CropArea, Point, Rect, Size};
use encoder::{Encoder, VideoEncoder};
use pollster::FutureExt;
use std::path::Path;
use std::sync::mpsc;

use crabgrab::capturable_content::{CapturableContent, CapturableContentFilter};
use crabgrab::capture_stream::{CaptureConfig, CapturePixelFormat, CaptureStream, StreamEvent};

// Variables to configure the stream
// Encoder configs are in the ./encoder folder
const STREAM_PX_FMT: CapturePixelFormat = CapturePixelFormat::Bgra8888;
const SCALE_FACTOR: f64 = 2.0; // NOTE: on macbooks this can be 2.0
const OUTPUT_FILE: &str = "./video.mp4";

fn main() -> Result<(), Error> {
    // MARK: Configure Stream
    let content = CapturableContent::new(CapturableContentFilter::DISPLAYS).block_on()?;
    let display = content
        .displays()
        .next()
        .ok_or(Error::msg("No displays found"))?;

    let size = display.rect().scaled(SCALE_FACTOR).size; // Hardcoded to 2 (as scale factor)
    let mut height = size.height;
    let mut width = size.width;

    let crop = CropArea {
        origin: Point {
            x: 1116.0,
            y: 309.0,
        },
        size: Size {
            width: 496.0,
            height: 440.0,
        },
        scale_factor: Some(SCALE_FACTOR),
    };

    width = crop.size.width * SCALE_FACTOR;
    height = crop.size.height * SCALE_FACTOR;

    let stream_cfg = CaptureConfig::with_display(display, STREAM_PX_FMT, None)
        .with_color_space_name("kCGColorSpaceSRGB".to_string())
        .with_crop_area(Some(crop))
        .with_output_size(size);

    let stream_token =
        CaptureStream::test_access(false).ok_or(Error::msg("Failed to get access token"))?;

    // MARK: Configure Encoder
    let output = Path::new(OUTPUT_FILE);
    let mut encoder = VideoEncoder::init(height, width, output)?;

    let (tx, rx) = mpsc::channel();

    let handle = std::thread::spawn(move || {
        while let Ok(Some(frame)) = rx.recv() {
            encoder.append_frame(frame).expect("couldn't encode frame");
        }

        encoder.finish().expect("couldn't finish encoding");
    });

    // MARK: Start stream
    let mut stream = CaptureStream::new(stream_token, stream_cfg, move |result| match result {
        Ok(event) => match event {
            StreamEvent::Video(frame) => {
                println!("got new frame");
                tx.send(Some(frame)).expect("couldn't send frame");
            }
            StreamEvent::End => match tx.send(None) {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Error sending end-of-stream signal: {}", e);
                }
            },
            _ => {}
        },
        Err(e) => eprintln!("Error: {}", e),
    })?;

    // MARK: Record for 3 seconds, then stop
    std::thread::sleep(std::time::Duration::from_secs(3));
    stream.stop()?;

    handle.join().expect("couldn't complete encoding thread");

    println!("finished!");

    Ok(())
}
