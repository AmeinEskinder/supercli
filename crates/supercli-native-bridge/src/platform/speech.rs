//! Speech recognition via `SFSpeechRecognizer`.
//!
//! Rust replacement for `DictationReflection.swift` and
//! `VoiceDictationController.swift` (iOS/macOS dictation). The Swift originals
//! drive on-device dictation for the composer, reflecting interim results and
//! committing a final transcript.
//!
//! The public surface is platform-agnostic: [`SpeechRecognizer`] streams
//! [`Transcription`] updates to a [`TranscriptionHandler`]. On Apple targets
//! this is backed by `SFSpeechRecognizer` + `AVAudioEngine`; elsewhere it is
//! a stub.

use super::PlatformError;

/// A transcription update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcription {
    /// The current best text (interim or final).
    pub text: String,
    /// `true` for the final result of an utterance.
    pub is_final: bool,
}

/// Receives transcription updates. Must be `Send`; updates may arrive on a
/// platform audio callback thread. Implementations should be quick and avoid
/// blocking.
pub trait TranscriptionHandler: Send + Sync {
    fn on_transcription(&self, transcription: Transcription);
}

/// Locale identifier for recognition (e.g. `"en-US"`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechLocale(pub String);

impl SpeechLocale {
    pub fn system_default() -> Self {
        Self("en-US".to_owned())
    }
}

/// Speech recognizer. Construct with [`SpeechRecognizer::new`].
pub struct SpeechRecognizer {
    inner: Inner,
}

enum Inner {
    #[cfg(target_vendor = "apple")]
    Native(native::NativeSpeechRecognizer),
    #[cfg(not(target_vendor = "apple"))]
    Stub,
}

impl SpeechRecognizer {
    pub fn new() -> Self {
        Self {
            inner: {
                #[cfg(target_vendor = "apple")]
                {
                    Inner::Native(native::NativeSpeechRecognizer::new())
                }
                #[cfg(not(target_vendor = "apple"))]
                {
                    Inner::Stub
                }
            },
        }
    }

    /// Request speech-recognition authorization.
    pub fn request_authorization(&self) -> Result<bool, PlatformError> {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(r) => r.request_authorization(),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => Err(PlatformError::UnsupportedPlatform("speech recognition")),
        }
    }

    /// Whether on-device recognition is available (locale support, etc.).
    pub fn is_available(&self, locale: &SpeechLocale) -> bool {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(r) => r.is_available(locale),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => {
                let _ = locale;
                false
            }
        }
    }

    /// Start streaming transcriptions for `locale` to `handler`.
    /// Calling `start` while already running replaces the handler.
    pub fn start(
        &self,
        locale: &SpeechLocale,
        handler: Box<dyn TranscriptionHandler>,
    ) -> Result<(), PlatformError> {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(r) => r.start(locale, handler),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => {
                let _ = (locale, handler);
                Err(PlatformError::UnsupportedPlatform("speech recognition"))
            }
        }
    }

    /// Stop the current recognition task, if any.
    pub fn stop(&self) {
        match &self.inner {
            #[cfg(target_vendor = "apple")]
            Inner::Native(r) => r.stop(),
            #[cfg(not(target_vendor = "apple"))]
            Inner::Stub => {}
        }
    }
}

impl Default for SpeechRecognizer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_vendor = "apple")]
mod native {
    //! objc2 bindings for `SFSpeechRecognizer`.
    //!
    //! `VoiceDictationController.swift` wires `SFSpeechRecognizer` +
    //! `SFSpeechAudioBufferRecognitionRequest` + `AVAudioEngine`, forwarding
    //! `SFTranscription` best-transcription updates. The full audio-engine
    //! tap is AVFoundation work; this slice binds the recognizer lifecycle
    //! (authorization, availability, task start/cancel) and leaves the audio
    //! tap as the documented next step.

    use super::{SpeechLocale, Transcription, TranscriptionHandler};
    use crate::platform::PlatformError;
    use block2::Block;
    use objc2::rc::Retained;
    use objc2_foundation::NSLocale;
    use objc2_speech::{SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus};
    use std::sync::Mutex;

    pub(super) struct NativeSpeechRecognizer {
        handler: Mutex<Option<Box<dyn TranscriptionHandler>>>,
        // The live recognition task, if any. Kept opaque: cancelling it
        // stops audio delivery.
        task: Mutex<Option<Retained<objc2_speech::SFSpeechRecognitionTask>>>,
    }

    impl NativeSpeechRecognizer {
        pub(super) fn new() -> Self {
            Self {
                handler: Mutex::new(None),
                task: Mutex::new(None),
            }
        }

        fn recognizer_for(
            locale: &SpeechLocale,
        ) -> Result<Retained<SFSpeechRecognizer>, PlatformError> {
            let ns_locale = NSLocale::localeWithLocaleIdentifier(
                &objc2_foundation::NSString::from_str(&locale.0),
            );
            SFSpeechRecognizer::alloc()
                .initWithLocale(&ns_locale)
                .ok_or_else(|| {
                    PlatformError::InvalidInput(format!(
                        "speech recognition unavailable for locale {}",
                        locale.0
                    ))
                })
        }

        pub(super) fn request_authorization(&self) -> Result<bool, PlatformError> {
            use std::sync::mpsc::channel;
            let (tx, rx) = channel::<SFSpeechRecognizerAuthorizationStatus>();
            let block = Block::new(move |status: SFSpeechRecognizerAuthorizationStatus| {
                let _ = tx.send(status);
            });
            unsafe {
                SFSpeechRecognizer::requestAuthorization(&block);
            }
            let status = rx.recv().map_err(|_| {
                PlatformError::Platform("speech authorization reply lost".to_owned())
            })?;
            // SFSpeechRecognizerAuthorizationStatus: 0 NotDetermined, 1 Denied,
            // 2 Restricted, 3 Authorized.
            Ok(status.0 == 3)
        }

        pub(super) fn is_available(&self, locale: &SpeechLocale) -> bool {
            match Self::recognizer_for(locale) {
                Ok(recognizer) => recognizer.isAvailable(),
                Err(_) => false,
            }
        }

        pub(super) fn start(
            &self,
            locale: &SpeechLocale,
            handler: Box<dyn TranscriptionHandler>,
        ) -> Result<(), PlatformError> {
            // Cancel any in-flight task first (matches VoiceDictationController
            // restarting dictation per composer focus).
            self.stop();
            *self.handler.lock().unwrap() = Some(handler);

            let recognizer = Self::recognizer_for(locale)?;
            let request = objc2_speech::SFSpeechAudioBufferRecognitionRequest::new();
            // Partial (interim) results: mirrors DictationReflection's live
            // reflection of the in-progress utterance.
            request.setShouldReportPartialResults(true);

            // NOTE: the AVAudioEngine input tap that feeds `request` is the
            // remaining platform work (AVFoundation, out of scope for this
            // slice). Without audio input the task yields no results; the
            // lifecycle below is still the correct shape.
            let handler_cell = &self.handler;
            let task_cell = &self.task;
            let block = Block::new(
                move |result: *mut objc2_speech::SFSpeechRecognitionResult,
                      error: *mut objc2_foundation::NSError| {
                    if !result.is_null() {
                        let result = unsafe { &*result };
                        let text = result.bestTranscription().formattedString().to_string();
                        let transcription = Transcription {
                            text,
                            is_final: result.isFinal(),
                        };
                        if let Some(h) = handler_cell.lock().unwrap().as_ref() {
                            h.on_transcription(transcription);
                        }
                    }
                    if !error.is_null() || (!result.is_null() && unsafe { (*result).isFinal() }) {
                        // Terminal: drop the task handle.
                        *task_cell.lock().unwrap() = None;
                    }
                },
            );
            let task = recognizer
                .recognitionTaskWithRequest_resultHandler(&request, &block)
                .ok_or_else(|| {
                    PlatformError::Platform("could not start speech recognition task".to_owned())
                })?;
            *self.task.lock().unwrap() = Some(task);
            Ok(())
        }

        pub(super) fn stop(&self) {
            if let Some(task) = self.task.lock().unwrap().take() {
                task.cancel();
            }
            *self.handler.lock().unwrap() = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcription_fields() {
        let t = Transcription {
            text: "hello".into(),
            is_final: true,
        };
        assert!(t.is_final);
    }

    #[test]
    fn locale_default() {
        assert_eq!(SpeechLocale::system_default().0, "en-US");
    }

    /// Non-Apple targets report UnsupportedPlatform / unavailable.
    #[cfg(not(target_vendor = "apple"))]
    #[test]
    fn stub_reports_unsupported() {
        let r = SpeechRecognizer::new();
        assert!(!r.is_available(&SpeechLocale::system_default()));
        assert_eq!(
            r.request_authorization(),
            Err(PlatformError::UnsupportedPlatform("speech recognition"))
        );
        struct Noop;
        impl TranscriptionHandler for Noop {
            fn on_transcription(&self, _t: Transcription) {}
        }
        assert_eq!(
            r.start(&SpeechLocale::system_default(), Box::new(Noop)),
            Err(PlatformError::UnsupportedPlatform("speech recognition"))
        );
        // stop() must not panic on stubs.
        r.stop();
    }
}
