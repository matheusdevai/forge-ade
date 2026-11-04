// forge-voice — FORGE's native speech helper (macOS).
//
// Uses Apple's Speech framework (SFSpeechRecognizer) and AVAudioEngine.
// Talks JSON lines on stdout; reads "stop" on stdin. Audio is never written
// to disk by this helper.
//
//   forge-voice status
//   forge-voice authorize
//   forge-voice locales
//   forge-voice listen --locale pt-BR [--on-device] [--silence 1.6] [--max 60]
//   forge-voice file <path> --locale pt-BR [--on-device]

import AVFoundation
import Foundation
import Speech

setvbuf(stdout, nil, _IOLBF, 0)

func emit(_ obj: [String: Any]) {
    if let data = try? JSONSerialization.data(withJSONObject: obj, options: []), let s = String(data: data, encoding: .utf8) {
        print(s)
        fflush(stdout)
    }
}

func fail(_ code: String, _ message: String) -> Never {
    emit(["type": "error", "code": code, "message": message])
    exit(1)
}

func speechStatus() -> String {
    switch SFSpeechRecognizer.authorizationStatus() {
    case .authorized: return "authorized"
    case .denied: return "denied"
    case .restricted: return "restricted"
    case .notDetermined: return "notDetermined"
    @unknown default: return "unknown"
    }
}

func micStatus() -> String {
    switch AVCaptureDevice.authorizationStatus(for: .audio) {
    case .authorized: return "authorized"
    case .denied: return "denied"
    case .restricted: return "restricted"
    case .notDetermined: return "notDetermined"
    @unknown default: return "unknown"
    }
}

let args = Array(CommandLine.arguments.dropFirst())
func opt(_ name: String) -> String? {
    guard let i = args.firstIndex(of: name), i + 1 < args.count else { return nil }
    return args[i + 1]
}
let onDevice = args.contains("--on-device")
let localeId = opt("--locale") ?? "pt-BR"

func makeRecognizer() -> SFSpeechRecognizer {
    guard let r = SFSpeechRecognizer(locale: Locale(identifier: localeId)) else {
        fail("locale", "idioma não suportado pelo reconhecimento de fala: \(localeId)")
    }
    if !r.isAvailable { fail("unavailable", "reconhecimento de fala indisponível agora (sem rede ou serviço desativado)") }
    if onDevice && !r.supportsOnDeviceRecognition {
        fail("on-device", "o idioma \(localeId) não tem reconhecimento no dispositivo nesta máquina")
    }
    return r
}

func requireSpeech() {
    let s = speechStatus()
    if s != "authorized" { fail("speech-permission", "permissão de reconhecimento de fala: \(s)") }
}

switch args.first ?? "" {
case "status":
    emit(["type": "status", "speech": speechStatus(), "microphone": micStatus()])

case "authorize":
    let group = DispatchGroup()
    group.enter()
    SFSpeechRecognizer.requestAuthorization { _ in group.leave() }
    group.enter()
    AVCaptureDevice.requestAccess(for: .audio) { _ in group.leave() }
    DispatchQueue.global().async {
        group.wait()
        emit(["type": "status", "speech": speechStatus(), "microphone": micStatus()])
        exit(0)
    }
    dispatchMain()

case "locales":
    let items: [[String: Any]] = SFSpeechRecognizer.supportedLocales()
        .map { $0.identifier.replacingOccurrences(of: "_", with: "-") }
        .sorted()
        .map { id in
            let r = SFSpeechRecognizer(locale: Locale(identifier: id))
            return ["id": id, "onDevice": r?.supportsOnDeviceRecognition ?? false]
        }
    emit(["type": "locales", "locales": items])

case "file":
    requireSpeech()
    guard args.count >= 2 else { fail("usage", "informe o arquivo") }
    let url = URL(fileURLWithPath: args[1])
    let recognizer = makeRecognizer()
    let req = SFSpeechURLRecognitionRequest(url: url)
    req.shouldReportPartialResults = false
    if onDevice { req.requiresOnDeviceRecognition = true }
    if #available(macOS 13, *) { req.addsPunctuation = true }
    _ = recognizer.recognitionTask(with: req) { result, error in
        if let r = result, r.isFinal {
            emit(["type": "final", "text": r.bestTranscription.formattedString])
            exit(0)
        }
        if let e = error {
            fail("recognition", e.localizedDescription)
        }
    }
    dispatchMain()

case "listen":
    requireSpeech()
    if micStatus() != "authorized" { fail("mic-permission", "permissão de microfone: \(micStatus())") }
    let silence = Double(opt("--silence") ?? "1.6") ?? 1.6
    let maxSeconds = Double(opt("--max") ?? "60") ?? 60
    let recognizer = makeRecognizer()
    let request = SFSpeechAudioBufferRecognitionRequest()
    request.shouldReportPartialResults = true
    if onDevice { request.requiresOnDeviceRecognition = true }
    if #available(macOS 13, *) { request.addsPunctuation = true }

    let engine = AVAudioEngine()
    let input = engine.inputNode
    let format = input.outputFormat(forBus: 0)
    if format.sampleRate == 0 || format.channelCount == 0 { fail("no-input", "nenhum microfone disponível") }

    let lock = NSLock()
    var lastText = ""
    var lastChange = Date()
    let started = Date()
    var ended = false
    var lastLevel = Date.distantPast

    func endAudio(_ reason: String) {
        lock.lock()
        if ended { lock.unlock(); return }
        ended = true
        lock.unlock()
        engine.stop()
        input.removeTap(onBus: 0)
        request.endAudio()
        emit(["type": "ended", "reason": reason])
        // Safety net: never hang if the final result does not arrive.
        DispatchQueue.main.asyncAfter(deadline: .now() + 6) {
            emit(["type": "final", "text": lastText])
            exit(0)
        }
    }

    input.installTap(onBus: 0, bufferSize: 1024, format: format) { buffer, _ in
        request.append(buffer)
        guard let ch = buffer.floatChannelData?[0] else { return }
        let n = Int(buffer.frameLength)
        if n == 0 { return }
        var sum: Float = 0
        for i in 0..<n { sum += ch[i] * ch[i] }
        let rms = sqrt(sum / Float(n))
        let now = Date()
        if now.timeIntervalSince(lastLevel) > 0.08 {
            lastLevel = now
            emit(["type": "level", "rms": Double(min(1, rms * 8))])
        }
    }

    _ = recognizer.recognitionTask(with: request) { result, error in
        if let r = result {
            let text = r.bestTranscription.formattedString
            if r.isFinal {
                emit(["type": "final", "text": text])
                exit(0)
            }
            if text != lastText {
                lastText = text
                lastChange = Date()
                emit(["type": "partial", "text": text])
            }
        }
        if let e = error as NSError? {
            // 1110 = no speech detected
            if e.code == 1110 || e.code == 216 || e.code == 301 {
                emit(["type": "final", "text": lastText])
                exit(0)
            }
            fail("recognition", e.localizedDescription)
        }
    }

    engine.prepare()
    do { try engine.start() } catch { fail("audio", "não foi possível abrir o microfone: \(error.localizedDescription)") }
    emit(["type": "listening", "locale": localeId, "onDevice": onDevice])

    // Stop on "stop" / EOF from the parent.
    DispatchQueue.global().async {
        while let line = readLine() {
            if line.trimmingCharacters(in: .whitespaces) == "stop" { break }
        }
        DispatchQueue.main.async { endAudio("stopped") }
    }
    // Silence / max-duration watchdog.
    let watchdog = DispatchSource.makeTimerSource(queue: .main)
    watchdog.schedule(deadline: .now() + 0.2, repeating: 0.2)
    watchdog.setEventHandler {
        let now = Date()
        if now.timeIntervalSince(started) > maxSeconds { endAudio("max-duration"); return }
        if !lastText.isEmpty && now.timeIntervalSince(lastChange) > silence { endAudio("silence"); return }
        if lastText.isEmpty && now.timeIntervalSince(started) > 10 { endAudio("no-speech") }
    }
    watchdog.resume()
    dispatchMain()

default:
    fail("usage", "uso: forge-voice status|authorize|locales|listen|file")
}
