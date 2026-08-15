//! Real macOS microphone capture via `AVAudioRecorder`.
//!
//! The recorder NSObject is allocated, started and stopped entirely within a
//! private worker thread. `objc2`'s `Retained` handles are `!Send` on purpose,
//! so confining them to one thread is what keeps the capture path free of data
//! races; the `PlatformMicrophone` handle the service holds only owns an `mpsc`
//! sender and a `JoinHandle`, both `Send`.

use super::super::{CapturedAudio, DictationError, Microphone};
use objc2::runtime::{AnyClass, AnyObject, BOOL, NO, YES};
use objc2::{msg_send, rc::Retained};
use std::ffi::CStr;
use std::ffi::CString;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[link(name = "AVFoundation", kind = "framework")]
extern "C" {}

/// `'lpcm'` — `kAudioFormatLinearPCM`.
const LINEAR_PCM: isize = 0x6C70_636D;

fn objc_class(name: &CStr) -> &'static AnyClass {
    AnyClass::get(name).unwrap_or_else(|| {
        panic!(
            "Objective-C class `{}` is not available on this system",
            name.to_string_lossy()
        )
    })
}

fn ns_string(s: &str) -> Retained<AnyObject> {
    let cls = objc_class(c"NSString");
    let cstr = CString::new(s).expect("string contains a NUL byte");
    unsafe { msg_send![cls, stringWithUTF8String: cstr.as_ptr()] }
}

fn ns_number_int(value: isize) -> Retained<AnyObject> {
    let cls = objc_class(c"NSNumber");
    unsafe { msg_send![cls, numberWithInt: value] }
}

fn ns_number_double(value: f64) -> Retained<AnyObject> {
    let cls = objc_class(c"NSNumber");
    unsafe { msg_send![cls, numberWithDouble: value] }
}

fn ns_number_bool(value: bool) -> Retained<AnyObject> {
    let cls = objc_class(c"NSNumber");
    let b: BOOL = if value { YES } else { NO };
    unsafe { msg_send![cls, numberWithBool: b] }
}

fn ns_error_description(err: &Retained<AnyObject>) -> String {
    let desc: Retained<AnyObject> = unsafe { msg_send![err, description] };
    let ptr: *const std::os::raw::c_char = unsafe { msg_send![&desc, UTF8String] };
    if ptr.is_null() {
        return "unknown audio error".to_string();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

enum Setup {
    Ready,
    Failed(String),
}

enum Control {
    Stop,
    Cancel,
}

struct SessionEnd {
    audio: Vec<u8>,
    cancelled: bool,
    error: Option<String>,
}

pub struct PlatformMicrophone {
    output_path: PathBuf,
    control_tx: Option<mpsc::Sender<Control>>,
    handle: Option<thread::JoinHandle<SessionEnd>>,
    permission: bool,
}

impl PlatformMicrophone {
    pub fn new() -> Result<Self, DictationError> {
        let status = Self::authorization_status();
        if status == 2 || status == 1 {
            return Err(DictationError::PermissionDenied);
        }
        let permission = status == 3 || status == 0;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| {
                DictationError::CaptureInitFailed("system clock is before the UNIX epoch".into())
            })?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nabu-dictation-{}-{}.wav",
            std::process::id(),
            now
        ));
        Ok(Self {
            output_path: path,
            control_tx: None,
            handle: None,
            permission,
        })
    }

    fn authorization_status() -> isize {
        let cls = objc_class(c"AVCaptureDevice");
        let media = ns_string("audio");
        unsafe { msg_send![cls, authorizationStatusForMediaType: &*media] }
    }

    fn build_settings() -> Retained<AnyObject> {
        let cls = objc_class(c"NSMutableDictionary");
        let dict: Retained<AnyObject> = unsafe { msg_send![cls, new] };
        Self::set(&dict, "AVFormatIDKey", ns_number_int(LINEAR_PCM));
        Self::set(&dict, "AVSampleRateKey", ns_number_double(16000.0));
        Self::set(&dict, "AVNumberOfChannelsKey", ns_number_int(1));
        Self::set(&dict, "AVLinearPCMBitDepth", ns_number_int(16));
        Self::set(&dict, "AVLinearPCMIsFloatKey", ns_number_bool(false));
        Self::set(&dict, "AVLinearPCMIsNonInterleaved", ns_number_bool(false));
        dict
    }

    fn set(dict: &Retained<AnyObject>, key: &str, value: Retained<AnyObject>) {
        let k = ns_string(key);
        let _: () = unsafe { msg_send![&**dict, setObject: &*value forKey: &*k] };
    }

    fn file_url(path: &PathBuf) -> Retained<AnyObject> {
        let cls = objc_class(c"NSURL");
        let ns_path = ns_string(&path.to_string_lossy());
        unsafe { msg_send![cls, fileURLWithPath: &*ns_path] }
    }

    fn configure_session() -> Result<(), String> {
        let cls = objc_class(c"AVAudioSession");
        let session: Retained<AnyObject> = unsafe { msg_send![cls, sharedInstance] };
        let category = ns_string("AVAudioSessionCategoryPlayAndRecord");
        let res: Result<(), Retained<AnyObject>> =
            unsafe { msg_send![&session, setCategory: &*category error: _] };
        if let Err(e) = res {
            return Err(format!(
                "AVAudioSession category failed: {}",
                ns_error_description(&e)
            ));
        }
        let res: Result<(), Retained<AnyObject>> =
            unsafe { msg_send![&session, setActive: YES error: _] };
        res.map_err(|e| format!("AVAudioSession active failed: {}", ns_error_description(&e)))
    }

    fn create_recorder(path: &PathBuf) -> Result<Retained<AnyObject>, String> {
        let url = Self::file_url(path);
        let settings = Self::build_settings();
        let cls = objc_class(c"AVAudioRecorder");
        let alloc: Retained<AnyObject> = unsafe { msg_send![cls, alloc] };
        let delegate: Option<Retained<AnyObject>> = None;
        let rec: Option<Retained<AnyObject>> = unsafe {
            msg_send![&alloc, initWithURL: &*url settings: &*settings delegate: delegate]
        };
        rec.ok_or_else(|| "AVAudioRecorder failed to initialise".to_string())
    }

    fn drain_file(path: &PathBuf) -> Vec<u8> {
        for _ in 0..100 {
            if let Ok(meta) = std::fs::metadata(path) {
                if meta.len() >= 44 {
                    break;
                }
            }
            thread::sleep(Duration::from_millis(10));
        }
        std::fs::read(path).unwrap_or_default()
    }

    fn run(path: PathBuf, setup_tx: mpsc::SyncSender<Setup>, rx: mpsc::Receiver<Control>) -> SessionEnd {
        if let Err(e) = Self::configure_session() {
            let _ = setup_tx.send(Setup::Failed(e));
            return SessionEnd { audio: Vec::new(), cancelled: false, error: Some(e) };
        }
        let recorder = match Self::create_recorder(&path) {
            Ok(r) => r,
            Err(e) => {
                let _ = setup_tx.send(Setup::Failed(e.clone()));
                return SessionEnd { audio: Vec::new(), cancelled: false, error: Some(e) };
            }
        };
        let _: Result<(), Retained<AnyObject>> = unsafe { msg_send![&recorder, prepareToRecord] };
        let started: BOOL = unsafe { msg_send![&recorder, record] };
        if started == NO {
            let e = "microphone could not start recording".to_string();
            let _ = setup_tx.send(Setup::Failed(e.clone()));
            return SessionEnd { audio: Vec::new(), cancelled: false, error: Some(e) };
        }
        let _ = setup_tx.send(Setup::Ready);

        match rx.recv() {
            Ok(Control::Stop) => {
                let _: () = unsafe { msg_send![&recorder, stop] };
                let bytes = Self::drain_file(&path);
                let _ = std::fs::remove_file(&path);
                SessionEnd { audio: bytes, cancelled: false, error: None }
            }
            _ => {
                let _: () = unsafe { msg_send![&recorder, stop] };
                let _ = std::fs::remove_file(&path);
                SessionEnd { audio: Vec::new(), cancelled: true, error: None }
            }
        }
    }
}

impl Microphone for PlatformMicrophone {
    fn permission_granted(&self) -> bool {
        self.permission
    }

    fn open(&mut self) -> Result<(), DictationError> {
        Ok(())
    }

    fn start(&mut self) -> Result<(), DictationError> {
        if self.handle.is_some() {
            return Err(DictationError::AlreadyActive);
        }
        let (setup_tx, setup_rx) = mpsc::sync_channel::<Setup>(1);
        let (control_tx, control_rx) = mpsc::channel();
        let path = self.output_path.clone();
        let handle = thread::Builder::new()
            .name("nabu-dictation".into())
            .spawn(move || Self::run(path, setup_tx, control_rx))
            .map_err(|e| DictationError::CaptureInitFailed(format!("worker thread: {e}")))?;
        self.handle = Some(handle);
        match setup_rx.recv() {
            Ok(Setup::Ready) => {
                self.control_tx = Some(control_tx);
                Ok(())
            }
            Ok(Setup::Failed(e)) | Err(_) => {
                self.control_tx = Some(control_tx);
                if let Some(h) = self.handle.take() {
                    let _ = h.join();
                }
                Err(DictationError::CaptureInitFailed(e))
            }
        }
    }

    fn stop(&mut self) -> Result<CapturedAudio, DictationError> {
        let tx = self.control_tx.take().ok_or(DictationError::NotActive)?;
        let handle = self.handle.take().ok_or(DictationError::NotActive)?;
        let _ = tx.send(Control::Stop);
        let end = handle.join().map_err(|_| DictationError::CaptureInterrupted)?;
        if let Some(e) = end.error {
            return Err(DictationError::CaptureInitFailed(e));
        }
        if end.cancelled {
            return Err(DictationError::Cancelled);
        }
        if end.audio.is_empty() {
            return Err(DictationError::EmptyAudio);
        }
        Ok(CapturedAudio {
            wav_bytes: end.audio,
            sample_rate: 16000,
            channels: 1,
        })
    }

    fn cancel(&mut self) {
        if let Some(tx) = self.control_tx.take() {
            let _ = tx.send(Control::Cancel);
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for PlatformMicrophone {
    fn drop(&mut self) {
        self.cancel();
    }
}

impl Default for PlatformMicrophone {
    fn default() -> Self {
        Self::new().expect("PlatformMicrophone::default must not be used in production")
    }
}
