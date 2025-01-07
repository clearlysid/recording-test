mod encoder;

use anyhow::Error;
use crabgrab::prelude::WindowsDx11VideoFrame;
use encoder::win::{ContainerSettingsBuilder, ContainerSettingsSubType, SendDirectX, VideoSettingsBuilder, VideoSettingsSubType};
use windows::Graphics::DirectX::Direct3D11::IDirect3DSurface;
use std::sync::mpsc;
use std::path::Path;
use encoder::Encoder;
use pollster::FutureExt;

use crabgrab::capturable_content::{CapturableContent, CapturableContentFilter};
use crabgrab::capture_stream::{CaptureConfig, CaptureStream, StreamEvent, CapturePixelFormat};


const OUTPUT_FILE: &str = "./video.mp4";

fn main() -> Result<(), Error> {

    // MARK: Configure Stream
    let content = CapturableContent::new(CapturableContentFilter::DISPLAYS).block_on()?;
    let display = content.displays().next().ok_or(Error::msg("No displays found"))?;

    let size = display.rect().size; 
    let stream_cfg = CaptureConfig::with_display(display, CapturePixelFormat::Bgra8888, None)
        .with_output_size(size);
    // B8G8R8A8UIntNormalized
    let stream_token =
    CaptureStream::test_access(false).ok_or(Error::msg("Failed to get access token"))?;

    // MARK: Configure Encoder
    let output = Path::new(OUTPUT_FILE);

    let video_settings = VideoSettingsBuilder::new(1920, 1080)
    .frame_rate(60)
    .bitrate(150000)
    .sub_type(VideoSettingsSubType::H264);

    let container_settings = ContainerSettingsBuilder::new()
    .sub_type(ContainerSettingsSubType::MPEG4);

    let mut encoder = encoder::win::VideoEncoder::new(video_settings, container_settings, output)?;

    let (tx, rx) = mpsc::channel();
    
    let handle = std::thread::spawn(move || {
       
        while let Ok(source) = rx.recv() {
            println!("recevie frame");
            match source {
                Some((surface, ts)) => {
                    encoder.send_frame((surface, ts)).expect("couldn't encode frame");
                }
                None => {
                    println!("Finished!");
                    encoder.finish().expect("couldn't finish encoding");
                    break;
                }
            }
        }
        
    });
    // MARK: Start stream
    let mut stream = CaptureStream::new(stream_token, stream_cfg, move |result| match result {
        Ok(event) => match event {
            StreamEvent::Video(frame) => {
    
                let ts = frame.capture_time();
                let (surface, _) = frame.get_dx11_surface().expect("can't get surface");
                tx.send(Some((SendDirectX::new(surface), ts))).expect("couldn't send frame");
                println!("sent frame");
            },
            StreamEvent::End => {
                println!("stream end!");
                tx.send(None).expect("couldn't send none")
            },
            _ => {}
        },
        Err(e) => println!("Error: {}", e),
    })?;


    // MARK: Record for 3 seconds, then stop
    std::thread::sleep(std::time::Duration::from_secs(3));
    stream.stop()?;

    handle.join().expect("couldn't complete encoding thread");

    Ok(())
}
