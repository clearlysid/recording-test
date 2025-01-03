use std::{fs::{self, File}, path::Path, sync::{atomic::{self, AtomicBool}, mpsc, Arc}, thread::{self, JoinHandle}, time::Instant};
use ac_ffmpeg::codec::video::{frame, VideoFrame};

use windows::{core::{Error, HSTRING}, Foundation::{EventRegistrationToken, TimeSpan, TypedEventHandler}, Graphics::DirectX::Direct3D11::IDirect3DSurface, Media::{Core::{MediaStreamSample, MediaStreamSource, MediaStreamSourceSampleRequestedEventArgs, MediaStreamSourceStartingEventArgs, VideoStreamDescriptor}, MediaProperties::{ContainerEncodingProperties, MediaEncodingProfile, MediaEncodingSubtypes, VideoEncodingProperties}, Transcoding::MediaTranscoder}, Storage::{FileAccessMode, StorageFile}};

/// Used To Send DirectX Device Across Threads
pub struct SendDirectX<T>(pub T);

impl<T> SendDirectX<T> {
    /// Create A New `SendDirectX` Instance
    ///
    /// # Arguments
    ///
    /// * `device` - The DirectX Device
    ///
    /// # Returns
    ///
    /// Returns A New `SendDirectX` Instance
    #[must_use]
    #[inline]
    pub const fn new(device: T) -> Self {
        Self(device)
    }
}

#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<T> Send for SendDirectX<T> {}

pub struct EncoderWindowsRs {
    first_timespan : Option<Instant>,
    sample_requested : EventRegistrationToken,
    media_stream_source: MediaStreamSource,
    starting : EventRegistrationToken,
    transcode_thread : Option<JoinHandle<Result<(), Error>>>,
    error_notify: Arc<AtomicBool>,
    frame_sender: mpsc::Sender<Option<(SendDirectX<IDirect3DSurface>, TimeSpan)>>
}

impl EncoderWindowsRs {
    pub fn init(width: u32, height: u32, output: &Path) -> Result<EncoderWindowsRs, Error> {

        let media_encoding_profile = MediaEncodingProfile::new()?;

        let properties = VideoEncodingProperties::new()?;

        properties.SetSubtype(&HSTRING::from("HEVC"))?;
        properties.SetBitrate(15000000)?;
        properties.SetWidth(width)?;
        properties.SetHeight(height)?;
        properties.FrameRate()?.SetNumerator(60)?;
        properties.FrameRate()?.SetDenominator(1)?;
        properties
            .PixelAspectRatio()?
            .SetNumerator(1)?;
        properties
            .PixelAspectRatio()?
            .SetDenominator(1)?;

        media_encoding_profile.SetVideo(&properties)?;

        let container_encoding_properties = ContainerEncodingProperties::new()?;
        container_encoding_properties.SetSubtype(&HSTRING::from("MPEG4"))?;

        media_encoding_profile.SetContainer(&container_encoding_properties)?;

        let video_encoding_properties = VideoEncodingProperties::CreateUncompressed(&MediaEncodingSubtypes::Bgra8()?, properties.Width()?, properties.Height()?)?;

        let video_encoding_descriptor = VideoStreamDescriptor::Create(&video_encoding_properties)?;

        let media_stream_source = MediaStreamSource::CreateFromDescriptor(&video_encoding_descriptor)?;

        media_stream_source.SetBufferTime(TimeSpan::default())?;
        
        let starting = media_stream_source.Starting(&TypedEventHandler::<
            MediaStreamSource,
            MediaStreamSourceStartingEventArgs,
            >::new(move |_, stream_start| {
                let stream_start = stream_start
                .as_ref()
                .expect("MediaStreamSource Starting parameter was None This Should Not Happen.");
            
            stream_start
            .Request()?
            .SetActualStartPosition(TimeSpan { Duration: 0 })?;
        Ok(())
    }))?;
    
    let (frame_sender, frame_receiver) =
    mpsc::channel::<Option<(SendDirectX<IDirect3DSurface>, TimeSpan)>>();
    
    let sample_requested = media_stream_source.SampleRequested(&TypedEventHandler::<MediaStreamSource, MediaStreamSourceSampleRequestedEventArgs>::new({
        move |_, sample_requested | {
            let sample_requested = sample_requested.as_ref().expect("MediaStreamSource SampleRequested parameter was None This Should Not Happen.");
            
            let frame = match frame_receiver.recv() {
                Ok(frame) => frame,
                Err(e) => panic!("Failed to receive frame from frame sender: {e}"),
            };
            match frame {
                Some((source, timespan)) => {
                    println!("{}", timespan.Duration);
                    let sample = MediaStreamSample::CreateFromDirect3D11Surface(&source.0, timespan)?;
                    sample_requested.Request()?.SetSample(&sample)?;
                }
                None => {
                    sample_requested.Request()?.SetSample(None)?;
                }
            }
            
            
            Ok(())
        }
    }))?;
    
    let media_transcoder = MediaTranscoder::new()?;
    
    media_transcoder.SetHardwareAccelerationEnabled(true)?;
    
    File::create(output).expect("File not created");
    let output = fs::canonicalize(output).unwrap().to_string_lossy()[4..].to_string();
    let path = Path::new(&output);
    
    let path = &HSTRING::from(path.as_os_str().to_os_string());
    
    let file = StorageFile::GetFileFromPathAsync(path)?.get()?;
    let media_stream_output = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;
    
    let transcoder = media_transcoder.PrepareMediaStreamSourceTranscodeAsync(&media_stream_source, &media_stream_output, &media_encoding_profile)?
    .get()?;

let error_notify = Arc::new(AtomicBool::new(false));

let transcoder_thread = thread::spawn({
    let error_notify = error_notify.clone();
    
    move || -> Result<(), Error> {
        let result = transcoder.TranscodeAsync();
        
        if result.is_err() {
            error_notify.store(true, atomic::Ordering::Relaxed);
        }
        
        let _ = result?.get();
        
        drop(media_transcoder);
        
        Ok(())
    }
});
        Ok(Self {
            first_timespan : None,
            error_notify,
            frame_sender,
            sample_requested,
            media_stream_source,
            starting,
            transcode_thread : Some(transcoder_thread)
        })
    }

    pub fn send_frame(&mut self,  (surface, ts) : (SendDirectX<IDirect3DSurface>, Instant)) -> Result<(), Error> {
        if self.first_timespan.is_none() {
            self.first_timespan = Some(ts);
        }

        let pts_raw = ts.duration_since(self.first_timespan.unwrap()).as_nanos();

        let pts = TimeSpan{Duration : (pts_raw as i64 / 100 )};

        self.frame_sender.send(Some((surface, pts))).expect("Didn't sent");

        Ok(())
    }

    pub fn finish(mut self) -> Result<(), Error> {
        print!("Finished");
        self.frame_sender.send(None).unwrap();

        if let Some(transcode_thread) = self.transcode_thread.take() {
            transcode_thread
                .join()
                .expect("Failed to join transcode thread")?;
        }

        self.media_stream_source.RemoveStarting(self.starting)?;
        self.media_stream_source
            .RemoveSampleRequested(self.sample_requested)?;

        Ok(())
    }
}