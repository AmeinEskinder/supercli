//! On-device dictation via the Speech framework (`SFSpeechRecognizer`).
//!
//! Backend status: `objc2-speech` 0.3.2 has no `SpeechAnalyzer` binding
//! (the iOS 26 API), so the recognizer path below is the shipping
//! implementation. When a newer `objc2-speech` ships the analyzer, add a
//! second backend behind [`SpeechBackend`] — the JSON events the launcher
//! consumes (`partial` / `final` / `error` / `authorization`) stay the same.
//!
//! Flow: `request_authorization()` → `start()` streams partial results →
//! `stop()` ends the audio and delivers the final transcription. Only one
//! dictation session runs at a time; starting while active is an error
//! event, never a second engine.

/// Which Speech-framework backend produced the results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpeechBackend {
    /// Classic `SFSpeechRecognizer` (all supported iOS versions).
    SpeechRecognizer,
    /// `SpeechAnalyzer` (iOS 26+) — no `objc2` binding exists yet.
    /// Reserved variant; selecting it is an error until implemented.
    SpeechAnalyzer,
}

#[cfg(target_vendor = "apple")]
pub(crate) mod apple {
    use crate::{emit, json_escape};
    use block2::{DynBlock, StackBlock};
    use objc2::rc::{Allocated, Retained};
    use objc2::runtime::NSObject;
    use objc2::{extern_class, extern_methods, AnyThread};
    use objc2_foundation::NSError;
    use objc2_speech::{
        SFSpeechRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognizer,
        SFSpeechRecognizerAuthorizationStatus,
    };
    use std::sync::Mutex;

    // ------------------------------------------------------------------
    // Hand-declared bindings missing from objc2-speech / objc2-av-foundation
    // 0.3.2. These are stable, long-frozen selectors; objc2 validates the
    // signatures at compile time, which is exactly the check we want from
    // the Apple-target `cargo check` in CI. Buffer/format/time are opaque:
    // the bridge only passes them through.
    // ------------------------------------------------------------------

    extern_class!(
        #[unsafe(super(SFSpeechRecognitionRequest))]
        #[name = "SFSpeechAudioBufferRecognitionRequest"]
        pub struct AudioBufferRecognitionRequest;
    );

    #[allow(non_snake_case)]
    impl AudioBufferRecognitionRequest {
        extern_methods!(
            #[unsafe(method(init))]
            #[unsafe(method_family = init)]
            pub unsafe fn init(this: Allocated<Self>) -> Retained<Self>;

            #[unsafe(method(appendAudioPCMBuffer:))]
            #[unsafe(method_family = none)]
            pub unsafe fn appendAudioPCMBuffer(&self, buffer: *mut AVAudioPCMBuffer);

            #[unsafe(method(endAudio))]
            #[unsafe(method_family = none)]
            pub unsafe fn endAudio(&self);
        );
    }

    extern_class!(
        #[unsafe(super(NSObject))]
        #[name = "AVAudioEngine"]
        pub struct AVAudioEngine;
    );

    extern_class!(
        #[unsafe(super(NSObject))]
        #[name = "AVAudioNode"]
        struct AVAudioNode;
    );

    extern_class!(
        #[unsafe(super(AVAudioNode))]
        #[name = "AVAudioInputNode"]
        struct AVAudioInputNode;
    );

    extern_class!(
        #[unsafe(super(NSObject))]
        #[name = "AVAudioFormat"]
        struct AVAudioFormat;
    );

    extern_class!(
        #[unsafe(super(NSObject))]
        #[name = "AVAudioPCMBuffer"]
        struct AVAudioPCMBuffer;
    );

    extern_class!(
        #[unsafe(super(NSObject))]
        #[name = "AVAudioTime"]
        struct AVAudioTime;
    );

    #[allow(non_snake_case)]
    impl AVAudioEngine {
        extern_methods!(
            #[unsafe(method(init))]
            #[unsafe(method_family = init)]
            pub unsafe fn init(this: Allocated<Self>) -> Retained<Self>;

            #[unsafe(method(inputNode))]
            #[unsafe(method_family = none)]
            pub unsafe fn inputNode(&self) -> Retained<AVAudioInputNode>;

            #[unsafe(method(prepare))]
            #[unsafe(method_family = none)]
            pub unsafe fn prepare(&self);

            #[unsafe(method(startAndReturnError:))]
            #[unsafe(method_family = none)]
            pub unsafe fn startAndReturnError(&self, error: *mut *mut NSError) -> bool;

            #[unsafe(method(stop))]
            #[unsafe(method_family = none)]
            pub unsafe fn stop(&self);
        );
    }

    #[allow(non_snake_case)]
    impl AVAudioInputNode {
        extern_methods!(
            #[unsafe(method(outputFormatForBus:))]
            #[unsafe(method_family = none)]
            pub unsafe fn outputFormatForBus(&self, bus: u64) -> Retained<AVAudioFormat>;

            #[unsafe(method(installTapOnBus:bufferSize:format:block:))]
            #[unsafe(method_family = none)]
            pub unsafe fn installTapOnBus_bufferSize_format_block(
                &self,
                bus: u64,
                buffer_size: u32,
                format: Option<&AVAudioFormat>,
                block: &DynBlock<dyn Fn(*mut AVAudioPCMBuffer, *mut AVAudioTime)>,
            );

            #[unsafe(method(removeTapOnBus:))]
            #[unsafe(method_family = none)]
            pub unsafe fn removeTapOnBus(&self, bus: u64);
        );
    }

    fn auth_status_name(s: SFSpeechRecognizerAuthorizationStatus) -> &'static str {
        if s == SFSpeechRecognizerAuthorizationStatus::Authorized {
            "authorized"
        } else if s == SFSpeechRecognizerAuthorizationStatus::Denied {
            "denied"
        } else if s == SFSpeechRecognizerAuthorizationStatus::Restricted {
            "restricted"
        } else {
            "not_determined"
        }
    }

    struct ActiveDictation {
        engine: Retained<AVAudioEngine>,
        request: Retained<AudioBufferRecognitionRequest>,
        task: Retained<objc2_speech::SFSpeechRecognitionTask>,
    }

    // The framework objects are confined to start/stop (serialized on the
    // mutex); the tap block only appends buffers, which AVFoundation
    // documents as thread-safe.
    unsafe impl Send for ActiveDictation {}

    static ACTIVE: Mutex<Option<ActiveDictation>> = Mutex::new(None);

    fn error_event(message: &str) {
        let mut j =
            String::from("{\"kind\":\"error\",\"backend\":\"speech_recognizer\",\"message\":\"");
        json_escape(&mut j, message);
        j.push_str("\"}");
        emit(&j);
    }

    /// Ask the user for Speech-recognition authorization.
    pub fn request_authorization() {
        let block = StackBlock::new(|status: SFSpeechRecognizerAuthorizationStatus| {
            let mut j = String::from(
                "{\"kind\":\"authorization\",\"backend\":\"speech_recognizer\",\"status\":\"",
            );
            j.push_str(auth_status_name(status));
            j.push_str("\"}");
            emit(&j);
        });
        // SAFETY: the framework copies the block before returning.
        unsafe { SFSpeechRecognizer::requestAuthorization(&block) };
    }

    /// Start streaming dictation. Partial results arrive as
    /// `{"kind":"partial","text":"…"}`; the terminal one as `{"kind":"final"}`.
    pub fn start() {
        if ACTIVE.lock().unwrap().is_some() {
            error_event("dictation already active");
            return;
        }
        // SAFETY: every framework call follows its documented contract;
        // failures surface as error events, never panics.
        unsafe { start_inner() };
    }

    unsafe fn start_inner() {
        let recognizer: Retained<SFSpeechRecognizer> =
            match SFSpeechRecognizer::init(SFSpeechRecognizer::alloc()) {
                Some(r) => r,
                None => {
                    error_event("no speech recognizer for the current locale");
                    return;
                }
            };
        if !recognizer.isAvailable() {
            error_event("speech recognition service unavailable");
            return;
        }

        let request: Retained<AudioBufferRecognitionRequest> =
            AudioBufferRecognitionRequest::init(AudioBufferRecognitionRequest::alloc());
        request.setShouldReportPartialResults(true);

        let engine: Retained<AVAudioEngine> = AVAudioEngine::init(AVAudioEngine::alloc());
        let input = engine.inputNode();
        let format = input.outputFormatForBus(0);

        // The request outlives the engine (held by ActiveDictation, torn down
        // in stop() after the tap is removed), so the raw pointer the tap
        // block captures stays valid.
        let request_ptr: *const AudioBufferRecognitionRequest = Retained::as_ptr(&request);

        let tap = StackBlock::new(
            move |buffer: *mut AVAudioPCMBuffer, _when: *mut AVAudioTime| {
                if !buffer.is_null() {
                    // SAFETY: buffer is valid for the tap call; the request
                    // outlives the installed tap.
                    unsafe { (*request_ptr).appendAudioPCMBuffer(buffer) };
                }
            },
        );
        input.installTapOnBus_bufferSize_format_block(0, 1024, Some(&format), &tap);
        // The engine retains its copy of the tap block; the stack block can
        // end here (StackBlock is Copy; no drop needed).

        engine.prepare();
        let mut err: *mut NSError = std::ptr::null_mut();
        if !engine.startAndReturnError(&mut err) {
            let msg = if err.is_null() {
                "audio engine failed to start".to_string()
            } else {
                (*err).localizedDescription().to_string()
            };
            input.removeTapOnBus(0);
            error_event(&msg);
            return;
        }

        let result_block = StackBlock::new(
            |result: *mut SFSpeechRecognitionResult, err: *mut NSError| {
                if !err.is_null() {
                    let desc = unsafe { (*err).localizedDescription().to_string() };
                    error_event(&format!("recognition failed: {desc}"));
                    return;
                }
                if result.is_null() {
                    return;
                }
                let (text, kind) = unsafe {
                    let text = (*result).bestTranscription().formattedString().to_string();
                    let kind = if (*result).isFinal() {
                        "final"
                    } else {
                        "partial"
                    };
                    (text, kind)
                };
                let mut j = String::from("{\"kind\":\"");
                j.push_str(kind);
                j.push_str("\",\"backend\":\"speech_recognizer\",\"text\":\"");
                json_escape(&mut j, &text);
                j.push_str("\"}");
                emit(&j);
            },
        );

        let task = recognizer.recognitionTaskWithRequest_resultHandler(&request, &result_block);
        // The recognizer retains its copy of the result block; the stack
        // block can end here.

        *ACTIVE.lock().unwrap() = Some(ActiveDictation {
            engine,
            request,
            task,
        });
        emit("{\"kind\":\"started\",\"backend\":\"speech_recognizer\"}");
    }

    /// End the audio, tear down the tap/engine, and cancel the task.
    /// Ordered teardown mirrors Apple's sample flow: no more audio in, stop
    /// the engine, then cancel the task.
    pub fn stop() {
        let active = ACTIVE.lock().unwrap().take();
        let Some(active) = active else {
            error_event("no active dictation to stop");
            return;
        };
        unsafe {
            active.request.endAudio();
            active.engine.inputNode().removeTapOnBus(0);
            active.engine.stop();
            active.task.cancel();
        }
        emit("{\"kind\":\"stopped\",\"backend\":\"speech_recognizer\"}");
    }
}

#[cfg(not(target_vendor = "apple"))]
pub(crate) mod apple {
    use crate::emit;
    /// Inert stubs: identical signatures, no framework calls.
    pub fn request_authorization() {
        emit("{\"kind\":\"error\",\"backend\":\"speech_recognizer\",\"message\":\"unavailable: not an Apple target\"}");
    }
    pub fn start() {
        emit("{\"kind\":\"error\",\"backend\":\"speech_recognizer\",\"message\":\"unavailable: not an Apple target\"}");
    }
    pub fn stop() {
        emit("{\"kind\":\"error\",\"backend\":\"speech_recognizer\",\"message\":\"unavailable: not an Apple target\"}");
    }
}

pub use apple::{request_authorization, start, stop};
