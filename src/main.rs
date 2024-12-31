
mod encoder;

use std::path::Path;
use std::sync::mpsc;
use encoder::Encoder;

use anyhow::Error;
use pollster::FutureExt;

use crabgrab::capturable_content::{CapturableContent, CapturableContentFilter};
use crabgrab::capture_stream::{CaptureConfig, CaptureStream, StreamEvent, CapturePixelFormat};

fn main() -> Result<(), Error> {

    // MARK: Setup stream
    let stream_px_fmt = CapturePixelFormat::Bgra8888;
    let content = CapturableContent::new(CapturableContentFilter::DISPLAYS).block_on()?;

    let display = content.displays().next().ok_or(Error::msg("No displays found"))?;

    let size = display.rect().scaled(2.0).size; // Hardcoded to 2 (as scale factor)
    let height = size.height;
    let width = size.width;

    println!("height {}, width {}", height, width);

    let stream_cfg = CaptureConfig::with_display(display, stream_px_fmt, None)
        .with_output_size(size);

    let stream_token =
    CaptureStream::test_access(false).ok_or(Error::msg("Failed to get access token"))?;

    // MARK: Setup encoder in new thread
    let output = Path::new("./video.mp4");
    let mut encoder = encoder::acffmpeg::EncoderAcFfmpeg::init(height, width, output)?;

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
