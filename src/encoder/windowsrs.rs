use std::{fs::{self, File}, path::Path, sync::{atomic::{self, AtomicBool}, mpsc, Arc, Condvar, Mutex}, thread::{self, JoinHandle}, time::Instant};
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

pub struct ContainerSettingsBuilder {
    sub_type: ContainerSettingsSubType,
}

impl ContainerSettingsBuilder {
    pub const fn new() -> Self {
        Self {
            sub_type: ContainerSettingsSubType::MPEG4,
        }
    }

    pub const fn sub_type(mut self, sub_type: ContainerSettingsSubType) -> Self {
        self.sub_type = sub_type;
        self
    }

    fn build(self) -> Result<ContainerEncodingProperties, Error> {
        let properties = ContainerEncodingProperties::new()?;
        properties.SetSubtype(&self.sub_type.to_hstring())?;
        Ok(properties)
    }
}

impl Default for ContainerSettingsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum VideoSettingsSubType {
    ARGB32,
    BGRA8,
    D16,
    H263,
    H264,
    H264ES,
    HEVC,
    HEVCES,
    IYUV,
    L8,
    L16,
    MJPG,
    NV12,
    MPEG1,
    MPEG2,
    RGB24,
    RGB32,
    WMV3,
    WVC1,
    VP9,
    YUY2,
    YV12,
}

impl VideoSettingsSubType {
    pub fn to_hstring(&self) -> HSTRING {
        let s = match self {
            Self::ARGB32 => "ARGB32",
            Self::BGRA8 => "BGRA8",
            Self::D16 => "D16",
            Self::H263 => "H263",
            Self::H264 => "H264",
            Self::H264ES => "H264ES",
            Self::HEVC => "HEVC",
            Self::HEVCES => "HEVCES",
            Self::IYUV => "IYUV",
            Self::L8 => "L8",
            Self::L16 => "L16",
            Self::MJPG => "MJPG",
            Self::NV12 => "NV12",
            Self::MPEG1 => "MPEG1",
            Self::MPEG2 => "MPEG2",
            Self::RGB24 => "RGB24",
            Self::RGB32 => "RGB32",
            Self::WMV3 => "WMV3",
            Self::WVC1 => "WVC1",
            Self::VP9 => "VP9",
            Self::YUY2 => "YUY2",
            Self::YV12 => "YV12",
        };

        HSTRING::from(s)
    }
}

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum ContainerSettingsSubType {
    ASF,
    MP3,
    MPEG4,
    AVI,
    MPEG2,
    WAVE,
    AACADTS,
    ADTS,
    GP3,
    AMR,
    FLAC,
}

impl ContainerSettingsSubType {
    pub fn to_hstring(&self) -> HSTRING {
        match self {
            Self::ASF => HSTRING::from("ASF"),
            Self::MP3 => HSTRING::from("MP3"),
            Self::MPEG4 => HSTRING::from("MPEG4"),
            Self::AVI => HSTRING::from("AVI"),
            Self::MPEG2 => HSTRING::from("MPEG2"),
            Self::WAVE => HSTRING::from("WAVE"),
            Self::AACADTS => HSTRING::from("AACADTS"),
            Self::ADTS => HSTRING::from("ADTS"),
            Self::GP3 => HSTRING::from("3GP"),
            Self::AMR => HSTRING::from("AMR"),
            Self::FLAC => HSTRING::from("FLAC"),
        }
    }
}

pub struct VideoSettingsBuilder {
    sub_type: VideoSettingsSubType,
    bitrate: u32,
    width: u32,
    height: u32,
    frame_rate: u32,
    pixel_aspect_ratio: (u32, u32),
    disabled: bool,
}

impl VideoSettingsBuilder {
    pub const fn new(width: u32, height: u32) -> Self {
        Self {
            bitrate: 15000000,
            frame_rate: 60,
            pixel_aspect_ratio: (1, 1),
            sub_type: VideoSettingsSubType::HEVC,
            width,
            height,
            disabled: false,
        }
    }

    pub const fn sub_type(mut self, sub_type: VideoSettingsSubType) -> Self {
        self.sub_type = sub_type;
        self
    }

    pub const fn bitrate(mut self, bitrate: u32) -> Self {
        self.bitrate = bitrate;
        self
    }

    pub const fn width(mut self, width: u32) -> Self {
        self.width = width;
        self
    }

    pub const fn height(mut self, height: u32) -> Self {
        self.height = height;
        self
    }

    pub const fn frame_rate(mut self, frame_rate: u32) -> Self {
        self.frame_rate = frame_rate;
        self
    }

    pub const fn pixel_aspect_ratio(mut self, pixel_aspect_ratio: (u32, u32)) -> Self {
        self.pixel_aspect_ratio = pixel_aspect_ratio;
        self
    }

    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    fn build(self) -> Result<(VideoEncodingProperties, bool), Error> {
        let properties = VideoEncodingProperties::new()?;

        properties.SetSubtype(&self.sub_type.to_hstring())?;
        properties.SetBitrate(self.bitrate)?;
        properties.SetWidth(self.width)?;
        properties.SetHeight(self.height)?;
        properties.FrameRate()?.SetNumerator(self.frame_rate)?;
        properties.FrameRate()?.SetDenominator(1)?;
        properties
            .PixelAspectRatio()?
            .SetNumerator(self.pixel_aspect_ratio.0)?;
        properties
            .PixelAspectRatio()?
            .SetDenominator(self.pixel_aspect_ratio.1)?;

        Ok((properties, self.disabled))
    }
}

pub struct EncoderWindowsRs {
    first_timespan: Option<Instant>,
    frame_sender: mpsc::Sender<Option<(SendDirectX<IDirect3DSurface>, TimeSpan)>>,
    sample_requested: EventRegistrationToken,
    media_stream_source: MediaStreamSource,
    starting: EventRegistrationToken,
    transcode_thread: Option<JoinHandle<Result<(), Error>>>,
    frame_notify: Arc<(Mutex<bool>, Condvar)>,
    error_notify: Arc<AtomicBool>,
    is_video_disabled: bool,
}

impl EncoderWindowsRs {
//     pub fn init(width: u32, height: u32, output: &Path) -> Result<EncoderWindowsRs, Error> {

//         let media_encoding_profile = MediaEncodingProfile::new()?;

//         let properties = VideoEncodingProperties::new()?;

//         properties.SetSubtype(&HSTRING::from("HEVC"))?;
//         properties.SetBitrate(15000000)?;
//         properties.SetWidth(width)?;
//         properties.SetHeight(height)?;
//         properties.FrameRate()?.SetNumerator(60)?;
//         properties.FrameRate()?.SetDenominator(1)?;
//         properties
//             .PixelAspectRatio()?
//             .SetNumerator(1)?;
//         properties
//             .PixelAspectRatio()?
//             .SetDenominator(1)?;

//         media_encoding_profile.SetVideo(&properties)?;

//         let container_encoding_properties = ContainerEncodingProperties::new()?;
//         container_encoding_properties.SetSubtype(&HSTRING::from("MPEG4"))?;

//         media_encoding_profile.SetContainer(&container_encoding_properties)?;

//         let video_encoding_properties = VideoEncodingProperties::CreateUncompressed(&MediaEncodingSubtypes::Bgra8()?, properties.Width()?, properties.Height()?)?;

//         let video_encoding_descriptor = VideoStreamDescriptor::Create(&video_encoding_properties)?;

//         let media_stream_source = MediaStreamSource::CreateFromDescriptor(&video_encoding_descriptor)?;

//         media_stream_source.SetBufferTime(TimeSpan::default())?;
        
//         let starting = media_stream_source.Starting(&TypedEventHandler::<
//             MediaStreamSource,
//             MediaStreamSourceStartingEventArgs,
//             >::new(move |_, stream_start| {
//                 let stream_start = stream_start
//                 .as_ref()
//                 .expect("MediaStreamSource Starting parameter was None This Should Not Happen.");
            
//             stream_start
//             .Request()?
//             .SetActualStartPosition(TimeSpan { Duration: 0 })?;
//         Ok(())
//     }))?;
    
//     let (frame_sender, frame_receiver) =
//     mpsc::channel::<Option<(SendDirectX<IDirect3DSurface>, TimeSpan)>>();
    
//     let sample_requested = media_stream_source.SampleRequested(&TypedEventHandler::<MediaStreamSource, MediaStreamSourceSampleRequestedEventArgs>::new({
//         move |_, sample_requested | {
//             let sample_requested = sample_requested.as_ref().expect("MediaStreamSource SampleRequested parameter was None This Should Not Happen.");
            
//             let frame = match frame_receiver.recv() {
//                 Ok(frame) => frame,
//                 Err(e) => panic!("Failed to receive frame from frame sender: {e}"),
//             };
//             match frame {
//                 Some((source, timespan)) => {
//                     println!("{}", timespan.Duration);
//                     let sample = MediaStreamSample::CreateFromDirect3D11Surface(&source.0, timespan)?;
//                     sample_requested.Request()?.SetSample(&sample)?;
//                 }
//                 None => {
//                     sample_requested.Request()?.SetSample(None)?;
//                 }
//             }
            
            
//             Ok(())
//         }
//     }))?;
    
//     let media_transcoder = MediaTranscoder::new()?;
    
//     media_transcoder.SetHardwareAccelerationEnabled(true)?;
    
//     File::create(output).expect("File not created");
//     let output = fs::canonicalize(output).unwrap().to_string_lossy()[4..].to_string();
//     let path = Path::new(&output);
    
//     let path = &HSTRING::from(path.as_os_str().to_os_string());
    
//     let file = StorageFile::GetFileFromPathAsync(path)?.get()?;
//     let media_stream_output = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;
    
//     let transcoder = media_transcoder.PrepareMediaStreamSourceTranscodeAsync(&media_stream_source, &media_stream_output, &media_encoding_profile)?
//     .get()?;

// let error_notify = Arc::new(AtomicBool::new(false));

// let transcoder_thread = thread::spawn({
//     let error_notify = error_notify.clone();
    
//     move || -> Result<(), Error> {
//         let result = transcoder.TranscodeAsync();
        
//         if result.is_err() {
//             error_notify.store(true, atomic::Ordering::Relaxed);
//         }
        
//         let _ = result?.get();
        
//         drop(media_transcoder);
        
//         Ok(())
//     }
// });
//         Ok(Self {
//             first_timespan : None,
//             error_notify,
//             frame_sender,
//             sample_requested,
//             media_stream_source,
//             starting,
//             transcode_thread : Some(transcoder_thread)
//         })
//     }

    #[inline]
    pub fn new<P: AsRef<Path>>(
        video_settings: VideoSettingsBuilder,
        container_settings: ContainerSettingsBuilder,
        path: P,
    ) -> Result<Self, Error> {
        let path = path.as_ref();
        let media_encoding_profile = MediaEncodingProfile::new()?;

        let (video_encoding_properties, is_video_disabled) = video_settings.build()?;
        media_encoding_profile.SetVideo(&video_encoding_properties)?;
        
        let container_encoding_properties = container_settings.build()?;
        media_encoding_profile.SetContainer(&container_encoding_properties)?;

        let video_encoding_properties = VideoEncodingProperties::CreateUncompressed(
            &MediaEncodingSubtypes::Bgra8()?,
            video_encoding_properties.Width()?,
            video_encoding_properties.Height()?,
        )?;
        let video_stream_descriptor = VideoStreamDescriptor::Create(&video_encoding_properties)?;

        let media_stream_source = MediaStreamSource::CreateFromDescriptor(
            &video_stream_descriptor
        )?;
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

        let frame_notify = Arc::new((Mutex::new(false), Condvar::new()));

        let sample_requested = media_stream_source.SampleRequested(&TypedEventHandler::<
            MediaStreamSource,
            MediaStreamSourceSampleRequestedEventArgs,
        >::new({
            let frame_receiver = frame_receiver;
            let frame_notify = frame_notify.clone();

            move |_, sample_requested| {
                let sample_requested = sample_requested.as_ref().expect(
                    "MediaStreamSource SampleRequested parameter was None This Should Not Happen.",
                );
                    if is_video_disabled {
                        sample_requested.Request()?.SetSample(None)?;

                        return Ok(());
                    }

                    let frame = match frame_receiver.recv() {
                        Ok(frame) => frame,
                        Err(e) => panic!("Failed to receive frame from frame sender: {e}"),
                    };
                    println!("Frame received!");
                    match frame {
                        Some((source, timespan)) => {
                            println!("{:?}", source.0.Description()?.Width);
                            let sample = 
                            MediaStreamSample::CreateFromDirect3D11Surface(
                                &source.0, timespan,
                            )?;
                            
                            sample_requested.Request()?.SetSample(&sample)?;
                            println!("{}", sample.Duration()?.Duration);
                            },
                        None => {
                            sample_requested.Request()?.SetSample(None)?;
                        }
                    }

                let (lock, cvar) = &*frame_notify;
                if let Ok(mut guard) = lock.lock() {
                    *guard = true;
                    cvar.notify_one();
                } else {
                    eprintln!("Failed to acquire the mutex lock.");
                }

                Ok(())
            }
        }))?;

        let media_transcoder = MediaTranscoder::new()?;
        media_transcoder.SetHardwareAccelerationEnabled(true)?;

        File::create(path).expect("File no created");
        let path = fs::canonicalize(path).unwrap().to_string_lossy()[4..].to_string();
        let path = Path::new(&path);

        let path = &HSTRING::from(path.as_os_str().to_os_string());

        let file = StorageFile::GetFileFromPathAsync(path)?.get()?;
        let media_stream_output = file.OpenAsync(FileAccessMode::ReadWrite)?.get()?;

        let transcode = media_transcoder
            .PrepareMediaStreamSourceTranscodeAsync(
                &media_stream_source,
                &media_stream_output,
                &media_encoding_profile,
            )?
            .get()?;

        let error_notify = Arc::new(AtomicBool::new(false));
        let transcode_thread = thread::spawn({
            let error_notify = error_notify.clone();
            move || -> Result<(), Error> {
                let result = transcode.TranscodeAsync();

                if result.is_err() {
                    error_notify.store(true, atomic::Ordering::Relaxed);
                }

                result?.get()?;

                drop(media_transcoder);

                Ok(())
            }
        });

        Ok(Self {
            first_timespan: None,
            frame_sender,
            sample_requested,
            media_stream_source,
            starting,
            transcode_thread: Some(transcode_thread),
            frame_notify,
            error_notify,
            is_video_disabled,
        })
    }

    pub fn send_frame(&mut self,  (surface, ts) : (SendDirectX<IDirect3DSurface>, Instant)) -> Result<(), Error> {
        if self.first_timespan.is_none() {
            self.first_timespan = Some(ts);
        }

        let pts_raw = ts.duration_since(self.first_timespan.unwrap()).as_nanos();

        let pts = TimeSpan{Duration : (pts_raw as i64 / 100 )};

        self.frame_sender.send(Some((surface, pts))).expect("Didn't sent");

        let (lock, cvar) = &*self.frame_notify;
        let processed = lock.lock();

        if let Ok(mut guard) = processed {
            if !*guard {
                let _unused = cvar.wait(guard);
            } else {
                *guard = false;
                drop(guard);
            }
        }

        if self.error_notify.load(atomic::Ordering::Relaxed) {
            if let Some(transcode_thread) = self.transcode_thread.take() {
                transcode_thread
                    .join()
                    .expect("Failed to join transcode thread")?;
            }
        }

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