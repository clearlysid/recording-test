use anyhow::Error;

use std::path::Path;
use std::time::Instant;
use std::thread::JoinHandle;

use crate::Encoder;

use windows::core::HSTRING;
use windows::Foundation::{EventRegistrationToken, TimeSpan, TypedEventHandler};
use windows::Storage::{FileAccessMode, StorageFile};
use windows::Media::Transcoding::MediaTranscoder;
use windows::Media::Core::{
    MediaStreamSample, MediaStreamSource,
    MediaStreamSourceSampleRequestedEventArgs, MediaStreamSourceStartingEventArgs,
    VideoStreamDescriptor,
};
use windows::Media::MediaProperties::{
    ContainerEncodingProperties, MediaEncodingProfile,
    MediaEncodingSubtypes, VideoEncodingProperties,
};


pub struct WmfEncoder {
    first_ts: Option<Instant>,
    sample_tx: std::sync::mpsc::Sender<Option<MediaStreamSample>>,
    sample_requested: EventRegistrationToken,
    media_stream_source: MediaStreamSource,
    starting: EventRegistrationToken,
    transcode_thread: Option<JoinHandle<Result<(), Error>>>,
}

impl WmfEncoder {
    pub fn init(height: f64, width: f64, output: &Path) -> Result<Self, Error> {

        // Setup video properties
        let video_props = VideoEncodingProperties::new()?;
        video_props.SetSubtype(&HSTRING::from("H264"))?;
        video_props.SetBitrate(15_000_000)?;
        video_props.SetWidth(width as u32)?;
        video_props.SetHeight(height as u32)?;
        video_props.FrameRate()?.SetNumerator(60)?;
        video_props.FrameRate()?.SetDenominator(1)?;
        video_props.PixelAspectRatio()?.SetNumerator(1)?;
        video_props.PixelAspectRatio()?.SetDenominator(1)?;

        // Setup container properties (MPEG4 in our case)
        let container_props = ContainerEncodingProperties::new()?;
        container_props.SetSubtype(&HSTRING::from("MPEG4"))?;

        // Create a media profile from the above properties
        let media_profile = MediaEncodingProfile::new()?;
        media_profile.SetVideo(&video_props)?;
        media_profile.SetContainer(&container_props)?;

        // Here we create the "source" video props. Note the "uncompressed" tag + Bgra8 subtype.
        // NOTE: also has MJPEG, YUV, NV12 etc. interesting.
        let source_video_props = VideoEncodingProperties::CreateUncompressed(
            &MediaEncodingSubtypes::Bgra8()?,
            video_props.Width()?,
            video_props.Height()?,
        )?;
        let video_stream_descriptor = VideoStreamDescriptor::Create(&source_video_props)?;

        // Create a media stream source and set the buffer time
        let media_stream_source = MediaStreamSource::CreateFromDescriptor(&video_stream_descriptor)?;
        media_stream_source.SetBufferTime(TimeSpan::default())?;

        let starting = media_stream_source.Starting(&TypedEventHandler::<
            MediaStreamSource,
            MediaStreamSourceStartingEventArgs,
        >::new(move |_, stream_start| {
            let stream_start = stream_start
                .as_ref()
                .expect("how tf this none?");

            stream_start
                .Request()?
                .SetActualStartPosition(TimeSpan { Duration: 0 })?;
            Ok(())
        }))?;

        let (sample_tx, sample_rx) =
            std::sync::mpsc::channel::<Option<MediaStreamSample>>();

        let sample_requested = media_stream_source.SampleRequested(&TypedEventHandler::<
            MediaStreamSource,
            MediaStreamSourceSampleRequestedEventArgs,
        >::new({
            let sample_rx = sample_rx;

            move |_, sample_requested| {
                let sample_requested = sample_requested.as_ref().expect("how tf this none?");

                if let Some(sample) = sample_rx.recv().expect("couldn't get sample from sender") {
                    sample_requested.Request()?.SetSample(&sample)?;
                } else {
                    sample_requested.Request()?.SetSample(None)?;
                }

                Ok(())
            }
        }))?;

        let transcoder = MediaTranscoder::new()?;
        transcoder.SetHardwareAccelerationEnabled(true)?;

        std::fs::File::create(&output)?;
        let path = std::fs::canonicalize(&output).unwrap().to_string_lossy()[4..].to_string();
        let path = Path::new(&path);

        let path = &HSTRING::from(path.as_os_str().to_os_string());

        let file = StorageFile::GetFileFromPathAsync(path)?.get()?;
        let media_stream_output = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;

        let transcode = transcoder
            .PrepareMediaStreamSourceTranscodeAsync(
                &media_stream_source,
                &media_stream_output,
                &media_profile,
            )?
            .get()?;

        let transcode_thread = std::thread::spawn({
            move || -> Result<(), Error> {
                transcode.TranscodeAsync()?.get()?;
                drop(transcoder);
                Ok(())
            }
        });

        Ok(Self {
            first_ts: None,
            sample_tx,
            sample_requested,
            media_stream_source,
            starting,
            transcode_thread: Some(transcode_thread),
        })
    }
}



impl Encoder for WmfEncoder {
    fn append_frame(&mut self, frame: crabgrab::prelude::VideoFrame) -> Result<(), anyhow::Error> {
        // Process timestamp
        let ts = frame.capture_time();
        if self.first_ts.is_none() {
            self.first_ts = Some(ts)
        }

        // TOCHECK: this might be wrong, need to double check
        let ts_delta = ts.duration_since(self.first_ts.unwrap());
        let ts_delta_nanos = ts_delta.as_nanos() as i64;

        let timespan = TimeSpan { Duration: ts_delta_nanos };

        // Create a MediaStreamSample from D3DSurface
        use crabgrab::prelude::WindowsDx11VideoFrame;

        let (dx11_surface, _) = frame.get_dx11_surface()?;
        let media_sample = MediaStreamSample::CreateFromDirect3D11Surface(&dx11_surface, timespan)?;

        // Alt: create MediaStreamSample from Buffer
        // use crabgrab::prelude::{VideoFrameBitmap, FrameBitmap};
        // use windows::Security::Cryptography::CryptographicBuffer;

        // let media_sample = match frame.get_bitmap()? {
        //     FrameBitmap::BgraUnorm8x4(bgra_bytes) => {
        //         let buf = bgra_bytes.data.as_flattened();

        //         let buffer = CryptographicBuffer::CreateFromByteArray(buf)?;
        //         MediaStreamSample::CreateFromBuffer(&buffer, timespan)?
        //     },
        //     _ => unimplemented!("windows encoder no support this px format"),
        // };

        self.sample_tx.send(Some(media_sample)).expect("couldn't send sample");
        println!("sample sent to encoder w ts: {}", ts_delta_nanos);

        Ok(())
    }

    fn finish(&mut self) -> Result<(), anyhow::Error> {
        // Send empty sample
        self.sample_tx.send(None).expect("couldn't send no-op");

        // Conclude transcode thread
        if let Some(transcode_thread) = self.transcode_thread.take() {
            transcode_thread
                .join()
                .expect("Failed to join transcode thread")?;
        }

        // Close out stream source
        self.media_stream_source.RemoveStarting(self.starting)?;
        self.media_stream_source
            .RemoveSampleRequested(self.sample_requested)?;

        Ok(())
    }
}