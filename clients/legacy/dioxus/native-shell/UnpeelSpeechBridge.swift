// UnpeelSpeechBridge.swift
//
// Drop-in native speech driver for the Dioxus dictation contract. STATUS:
// unverified on Xcode — adapted line-by-line from the Swift client's
// VoiceDictationController.swift (same auth chain, same engine guards,
// same fallback order), never compiled here (no Mac on this VM). The
// ModernDictationBackend body below is a near-copy of the client's; the
// wiring around it (message names, cancel semantics) is new for the bridge.
//
// What the Rust side owns
// -----------------------
// unpeel-ui DictationView probes for `window.__unpeelNativeSpeech` at mount
// (NATIVE_SPEECH_PROBE_JS). When present, the Web Speech backend is NOT
// installed and mic/stop/cancel route here. The launcher owns the session
// state machine, the commit, and the reflection gate; this file only drives
// the recognizer and streams transcripts.
//
// Contract
// --------
// Install (in the shell's WKWebView configuration, before page load):
//
//     let speech = UnpeelSpeechBridge()
//     speech.attach(to: webView)   // adds the script message handler +
//                                  // injects window.__unpeelNativeSpeech
//
// JS entry point (injected by attach):
//
//     window.__unpeelNativeSpeech("start" | "stop" | "cancel")
//
// Messages back, all via dioxus.send (the launcher's pump parses them):
//   dictation:ready               — mic live, recording
//   dictation:unavailable         — no recognizer for this locale/device
//   dictation:ended               — backend died mid-recording (interruption,
//                                   recognizer failure). Sent ONLY on
//                                   unexpected death, never on clean
//                                   stop/cancel: the launcher commits from
//                                   its own state on stop.
//   dictation:error:microphone    — mic permission denied / input unavailable
//   dictation:error:recognizer    — speech auth denied / recognizer failed
//   dictation:ntext:{"final":0|1,"text":"..."} — live transcript, latest wins
//                                   (mirrors the analyzer's finalized +
//                                   volatile accumulation)
//
// Capability declarations: NSSpeechRecognitionUsageDescription +
// NSMicrophoneUsageDescription in Info.plist (see InfoPlistAdditions.plist).
//
// Semantics (mirror VoiceDictationController):
// - start: resets transcript, requests speech + mic auth (@Sendable
//   handlers — main-actor-inherited closures SIGTRAP on the TCC queue),
//   prefers SpeechAnalyzer + DictationTranscriber on iOS 26, falls back to
//   SFSpeechRecognizer on setup failure.
// - stop: ends audio cleanly, keeps the streamed transcript on the Rust
//   side; does NOT send dictation:ended.
// - cancel: abandons everything, drops the reflector, sends nothing.
// - Interruption (phone call / recognizer death mid-recording): sends
//   dictation:ended; the launcher keeps the transcript committable via
//   its paste-kept path.

import AVFoundation
import Speech
import UIKit
import WebKit

/// Drives the OS speech backends and streams results into the Dioxus
/// webview. Must be created on the main thread.
final class UnpeelSpeechBridge: NSObject, WKScriptMessageHandler {
    private weak var webView: WKWebView?
    private let engine = AVAudioEngine()

    // Legacy backend state.
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?

    // Modern backend (iOS 26+).
    private var modernBackend: ModernSpeechBackend?

    // In-flight reflection model, prewarmed while recording (owned by the
    // reflect bridge; the speech bridge only triggers the prewarm).
    weak var reflectBridge: UnpeelReflectBridge?

    /// Install the `unpeelNativeSpeech` message handler and inject
    /// `window.__unpeelNativeSpeech` at document start.
    func attach(to webView: WKWebView) {
        self.webView = webView
        let config = webView.configuration
        config.userContentController.add(self, name: "unpeelNativeSpeech")
        let source = """
            window.__unpeelNativeSpeech = function(cmd) {
              window.webkit.messageHandlers.unpeelNativeSpeech.postMessage(cmd);
            };
            """
        let script = WKUserScript(
            source: source,
            injectionTime: .atDocumentStart,
            forMainFrameOnly: true
        )
        config.userContentController.addUserScript(script)
        // Surface audio-session interruptions (phone calls) as dictation:ended.
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(audioSessionInterrupted(_:)),
            name: AVAudioSession.interruptionNotification,
            object: AVAudioSession.sharedInstance()
        )
    }

    // MARK: - WKScriptMessageHandler

    func userContentController(
        _: WKUserContentController,
        didReceive message: WKScriptMessage
    ) {
        guard let cmd = message.body as? String else { return }
        // The message handler fires off-main; hop to main for AVAudioSession.
        Task { @MainActor [weak self] in
            switch cmd {
            case "start": self?.start()
            case "stop": self?.stop()
            case "cancel": self?.cancel()
            default: break
            }
        }
    }

    // MARK: - Commands

    @MainActor
    private func start() {
        // Permission handlers fire on TCC/speech background queues and MUST
        // be @Sendable: a closure formed in a @MainActor context otherwise
        // inherits main-actor isolation and Swift 6 traps (SIGTRAP in
        // dispatch_assert_queue) the moment the system invokes it off-main.
        SFSpeechRecognizer.requestAuthorization { @Sendable [weak self] status in
            Task { @MainActor in
                guard let self else { return }
                guard status == .authorized else {
                    self.send("dictation:error:recognizer")
                    return
                }
                AVAudioApplication.requestRecordPermission { @Sendable granted in
                    Task { @MainActor in
                        guard granted else {
                            self?.send("dictation:error:microphone")
                            return
                        }
                        self?.beginRecognition()
                    }
                }
            }
        }
    }

    @MainActor
    private func stop() {
        // Clean stop: end audio, keep the streamed transcript on the Rust
        // side. No dictation:ended — the launcher commits from its own
        // state on stop.
        if let backend = modernBackend {
            backend.stop()
            modernBackend = nil
        }
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        request?.endAudio()
        task?.cancel()
        request = nil
        task = nil
        try? AVAudioSession.sharedInstance().setActive(
            false, options: .notifyOthersOnDeactivation)
    }

    @MainActor
    private func cancel() {
        // Abandon everything: no commit, no ended message.
        reflectBridge?.abandon()
        stop()
    }

    @objc
    private func audioSessionInterrupted(_ note: Notification) {
        guard let info = note.userInfo,
            let typeValue = info[AVAudioSessionInterruptionTypeKey] as? UInt,
            let type = AVAudioSession.InterruptionType(rawValue: typeValue),
            type == .began
        else { return }
        // Unexpected death mid-recording: the launcher keeps the transcript
        // committable via its paste-kept path.
        send("dictation:ended")
    }

    // MARK: - Recognition

    @MainActor
    private func beginRecognition() {
        // Warm the reflection model while the user talks, so the polish pass
        // adds minimal latency at stop time. No-op when Apple Intelligence
        // is unavailable.
        reflectBridge?.prewarm()
        if #available(iOS 26.0, *) {
            beginModernRecognition()
        } else {
            beginLegacyRecognition()
        }
    }

    @MainActor
    @available(iOS 26.0, *)
    private func beginModernRecognition() {
        let backend = ModernSpeechBackend(engine: engine)
        modernBackend = backend
        backend.onTranscript = { [weak self] text, isFinal in
            self?.sendText(text, isFinal: isFinal)
        }
        backend.onFailure = { [weak self] message, recoverable in
            Task { @MainActor in
                guard let self else { return }
                // Setup failure before we were live → retry on the classic
                // recognizer so the user still gets dictation. A mid-session
                // failure just surfaces as an interruption.
                if self.modernBackend == nil || !recoverable {
                    self.send("dictation:ended")
                } else {
                    self.modernBackend = nil
                    self.beginLegacyRecognition()
                }
                _ = message
            }
        }
        Task { @MainActor [weak self] in
            guard let self else { return }
            let started = await backend.start()
            if started {
                self.send("dictation:ready")
            }
            // If not started, onFailure already routed to legacy/ended.
        }
    }

    @MainActor
    private func beginLegacyRecognition() {
        guard let recognizer = SFSpeechRecognizer(), recognizer.isAvailable else {
            send("dictation:unavailable")
            return
        }
        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            guard session.isInputAvailable else {
                send("dictation:error:microphone")
                stop()
                return
            }

            let request = SFSpeechAudioBufferRecognitionRequest()
            request.shouldReportPartialResults = true
            self.request = request

            let input = engine.inputNode
            let format = input.outputFormat(forBus: 0)
            // installTap/engine.start CRASH (uncatchable ObjC exception) on
            // an invalid format. outputFormat alone is not a safe check: on
            // the Simulator (and right after permission granting) it can
            // report a cached 44.1kHz format while the hardware-side
            // inputFormat is 0Hz/0ch — start() then throws NSException.
            // Guard BOTH sides.
            let hardwareFormat = input.inputFormat(forBus: 0)
            guard format.sampleRate > 0, format.channelCount > 0,
                hardwareFormat.sampleRate > 0, hardwareFormat.channelCount > 0
            else {
                send("dictation:error:microphone")
                stop()
                return
            }
            input.removeTap(onBus: 0)
            // The tap fires on the audio queue: it must be @Sendable, or the
            // inherited main-actor isolation SIGTRAPs on the first buffer.
            // `append` is documented thread-safe, hence the unsafe capture.
            nonisolated(unsafe) let tapRequest = request
            input.installTap(onBus: 0, bufferSize: 1024, format: format) {
                @Sendable buffer, _ in
                tapRequest.append(buffer)
            }
            engine.prepare()
            try engine.start()

            // Same @Sendable requirement: results arrive on a speech
            // background queue. Extract before the hop —
            // SFSpeechRecognitionResult is not Sendable.
            task = recognizer.recognitionTask(with: request) {
                @Sendable [weak self] result, error in
                let text = result?.bestTranscription.formattedString
                let isFinal = result?.isFinal ?? false
                let failed = error != nil
                Task { @MainActor in
                    if let text { self?.sendText(text, isFinal: isFinal) }
                    if failed { self?.send("dictation:ended") }
                }
            }
            send("dictation:ready")
        } catch {
            send("dictation:error:microphone")
            stop()
        }
    }

    // MARK: - Messaging

    /// Latest-wins transcript. The whole message is JSON-encoded as one JS
    /// string literal, so arbitrary transcript text (quotes, newlines,
    /// emoji, single quotes) can never break out of it. The launcher
    /// parses it with parse_native_text.
    private func sendText(_ text: String, isFinal: Bool) {
        let payload: [String: Any] = ["final": isFinal ? 1 : 0, "text": text]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
            let json = String(data: data, encoding: .utf8),
            let message = Self.jsonLiteral("dictation:ntext:" + json)
        else { return }
        evaluate("dioxus.send(\(message))")
    }

    /// A bare `dictation:...` message (no user text inside).
    private func send(_ message: String) {
        guard let literal = Self.jsonLiteral(message) else { return }
        evaluate("dioxus.send(\(literal))")
    }

    private func evaluate(_ js: String) {
        DispatchQueue.main.async { [weak self] in
            self?.webView?.evaluateJavaScript(js, completionHandler: nil)
        }
    }

    /// A Swift string as a JS string literal (JSON string encoding: double
    /// quotes escaped, safe to embed in evaluateJavaScript source).
    /// Static so @Sendable callbacks can use it without touching self.
    private static func jsonLiteral(_ s: String) -> String? {
        guard let data = try? JSONSerialization.data(withJSONObject: [s]),
            var j = String(data: data, encoding: .utf8),
            j.count >= 2
        else { return nil }
        j.removeFirst()
        j.removeLast()
        return "\"" + j + "\""
    }
}

// MARK: - Modern backend (iOS 26 SpeechAnalyzer + DictationTranscriber)

/// iOS 26 on-device dictation. Near-verbatim adaptation of the Swift
/// client's ModernDictationBackend; only the callbacks changed (they now
/// hop to the bridge's sendText instead of a controller's transcript).
@available(iOS 26.0, *)
final class ModernSpeechBackend: @unchecked Sendable {
    /// Live transcript (finalized + current volatile), latest wins.
    var onTranscript: (@Sendable (String, _ isFinal: Bool) -> Void)?
    /// `recoverable` = the modern path failed to start and the caller should
    /// fall back to the classic recognizer.
    var onFailure: (@Sendable (String, _ recoverable: Bool) -> Void)?

    private let engine: AVAudioEngine
    private var analyzer: SpeechAnalyzer?
    private var transcriber: DictationTranscriber?
    private var inputContinuation: AsyncStream<AnalyzerInput>.Continuation?
    private var resultsTask: Task<Void, Never>?
    private var runTask: Task<Void, Never>?

    init(engine: AVAudioEngine) {
        self.engine = engine
    }

    /// Returns true once audio is flowing and the results loop is live. On
    /// any setup failure it invokes `onFailure(_, recoverable: true)` and
    /// returns false so the caller can fall back.
    func start() async -> Bool {
        let locale = Locale.current
        let supported = await DictationTranscriber.supportedLocales
        // Match on language+region; fall back to language-only.
        let hasLocale =
            supported.contains { $0.identifier(.bcp47) == locale.identifier(.bcp47) }
            || supported.contains {
                $0.language.languageCode == locale.language.languageCode
            }
        guard hasLocale else {
            onFailure?("Dictation isn't set up for this language", true)
            return false
        }

        let transcriber = DictationTranscriber(
            locale: locale,
            preset: .progressiveShortDictation
        )
        self.transcriber = transcriber
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        self.analyzer = analyzer

        guard
            let analyzerFormat = await SpeechAnalyzer.bestAvailableAudioFormat(
                compatibleWith: [transcriber])
        else {
            onFailure?("Dictation model unavailable", true)
            return false
        }

        do {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.record, mode: .measurement, options: .duckOthers)
            try session.setActive(true, options: .notifyOthersOnDeactivation)
            guard session.isInputAvailable else {
                onFailure?("Microphone unavailable", true)
                return false
            }

            let input = engine.inputNode
            let inputFormat = input.outputFormat(forBus: 0)
            let hardwareFormat = input.inputFormat(forBus: 0)
            guard inputFormat.sampleRate > 0, inputFormat.channelCount > 0,
                hardwareFormat.sampleRate > 0, hardwareFormat.channelCount > 0
            else {
                onFailure?("Microphone unavailable", true)
                return false
            }

            let (stream, continuation) = AsyncStream.makeStream(of: AnalyzerInput.self)
            self.inputContinuation = continuation

            let converter =
                inputFormat == analyzerFormat
                ? nil : AVAudioConverter(from: inputFormat, to: analyzerFormat)

            input.removeTap(onBus: 0)
            nonisolated(unsafe) let unsafeConverter = converter
            input.installTap(onBus: 0, bufferSize: 4096, format: inputFormat) {
                @Sendable buffer, _ in
                let toYield: AVAudioPCMBuffer
                if let unsafeConverter {
                    guard
                        let converted = Self.convert(
                            buffer, using: unsafeConverter, to: analyzerFormat)
                    else { return }
                    toYield = converted
                } else {
                    toYield = buffer
                }
                continuation.yield(AnalyzerInput(buffer: toYield))
            }
            engine.prepare()
            try engine.start()

            let onFailure = self.onFailure
            let onTranscript = self.onTranscript

            runTask = Task {
                do {
                    try await analyzer.start(inputSequence: stream)
                } catch {
                    onFailure?("Dictation interrupted", false)
                }
            }

            resultsTask = Task {
                var finalized = AttributedString()
                do {
                    for try await result in transcriber.results {
                        if result.isFinal {
                            finalized += result.text
                            onTranscript?(
                                String(finalized.characters), true)
                        } else {
                            let combined = finalized + result.text
                            onTranscript?(
                                String(combined.characters), false)
                        }
                    }
                } catch {
                    // End of stream / cancellation is normal on stop.
                }
            }
            return true
        } catch {
            onFailure?("Couldn't start the microphone", true)
            return false
        }
    }

    func stop() {
        engine.stop()
        engine.inputNode.removeTap(onBus: 0)
        inputContinuation?.finish()
        inputContinuation = nil
        if let analyzer {
            Task { try? await analyzer.finalizeAndFinishThroughEndOfInput() }
        }
        resultsTask?.cancel()
        runTask?.cancel()
        resultsTask = nil
        runTask = nil
        analyzer = nil
        transcriber = nil
    }

    /// Sample-rate/format convert one mic buffer into the analyzer's format.
    private static func convert(
        _ buffer: AVAudioPCMBuffer,
        using converter: AVAudioConverter,
        to format: AVAudioFormat
    ) -> AVAudioPCMBuffer? {
        let ratio = format.sampleRate / buffer.format.sampleRate
        let capacity = AVAudioFrameCount(Double(buffer.frameLength) * ratio) + 1024
        guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: capacity)
        else { return nil }
        var consumed = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if consumed {
                status.pointee = .noDataNow
                return nil
            }
            consumed = true
            status.pointee = .haveData
            return buffer
        }
        return error == nil ? output : nil
    }
}
