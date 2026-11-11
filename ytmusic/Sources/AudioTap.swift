import Accelerate
import AudioToolbox
import CoreAudio
import Foundation
import os

/// Captures what this app's web view plays with a Core Audio process tap (macOS 14.2+) and
/// turns it into the 64 spectrum bins and 64-point waveform Nightjar draws.
///
/// WebKit plays media from its own helper processes (GPU / WebContent), so the tap targets the
/// audio processes whose responsible process is this app: Safari's own audio is never captured.
/// Web Audio can't do this here: WebKit doesn't feed Media Source streams into an AnalyserNode.
/// The first capture asks for "system audio recording" permission.
final class AudioTap: @unchecked Sendable {
    static let window = 2048
    static let bins = 64
    static let wave = 64
    // The same band edges and dB range as Nightjar's own analyser (src/analysis.rs).
    static let loHz = 40.0, hiHz = 16_000.0, floorDB = -72.0, ceilDB = -12.0

    /// Test mode: the tapped audio still reaches the tap but not the speakers.
    let muted: Bool
    private(set) var status = "idle"

    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var procID: AudioDeviceIOProcID?
    private var tapped: [AudioObjectID] = []
    private var sampleRate = 48_000.0

    // Mono samples from the audio thread; a short lock, never held across a callback.
    private let lock = OSAllocatedUnfairLock()
    private var ring = [Float](repeating: 0, count: 1 << 15)
    private var head = 0
    private var lastRead = 0

    private let fft: FFTSetup
    private let hann: [Float]

    init(muted: Bool) {
        self.muted = muted
        fft = vDSP_create_fftsetup(vDSP_Length(log2(Double(Self.window))), FFTRadix(kFFTRadix2))!
        var w = [Float](repeating: 0, count: Self.window)
        vDSP_hann_window(&w, vDSP_Length(Self.window), Int32(vDSP_HANN_DENORM))
        hann = w
    }

    // MARK: Which processes

    /// `responsibility_get_pid_responsible_for_pid` maps a WebKit helper to the app it works
    /// for. It's exported by libSystem but not in the SDK headers, so look it up at run time.
    private static let responsible: (@convention(c) (pid_t) -> pid_t)? = {
        guard let sym = dlsym(UnsafeMutableRawPointer(bitPattern: -2), "responsibility_get_pid_responsible_for_pid") else { return nil }
        return unsafeBitCast(sym, to: (@convention(c) (pid_t) -> pid_t).self)
    }()

    /// Audio process objects that belong to this app (itself and its WebKit helpers).
    static func ownAudioProcesses() -> [AudioObjectID] {
        let me = getpid()
        let objects: [AudioObjectID] = getArray(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyProcessObjectList)
        return objects.filter { object in
            guard let pid: pid_t = get(object, kAudioProcessPropertyPID) else { return false }
            if pid == me { return true }
            if let responsible { return responsible(pid) == me }
            // No responsibility lookup: fall back to WebKit helpers by bundle ID.
            let bundle: CFString? = get(object, kAudioProcessPropertyBundleID)
            return (bundle as String?)?.hasPrefix("com.apple.WebKit") == true
        }.sorted()
    }

    /// Starts the tap, or moves it when this app's audio processes changed. Call about once a
    /// second: WebKit starts its GPU process on the first media and may restart it.
    func refresh() {
        let ours = Self.ownAudioProcesses()
        if ours.isEmpty {
            if !tapped.isEmpty { stop() }
            status = "waiting for audio"
            return
        }
        guard ours != tapped else { return }
        stop()
        tapped = ours
        do {
            try start(ours)
            status = "ok \(Int(sampleRate)) Hz, \(ours.count) process\(ours.count == 1 ? "" : "es")"
        } catch {
            status = "error: \(error)"
            stop()
        }
    }

    private struct Failure: Error, CustomStringConvertible {
        let step: String
        let code: OSStatus
        var description: String { "\(step) (\(code))" }
    }

    private func check(_ code: OSStatus, _ step: String) throws {
        if code != noErr { throw Failure(step: step, code: code) }
    }

    private func start(_ processes: [AudioObjectID]) throws {
        let description = CATapDescription(stereoMixdownOfProcesses: processes)
        description.uuid = UUID()
        description.name = "Nightjar YouTube"
        description.isPrivate = true
        description.muteBehavior = muted ? .muted : .unmuted
        try check(AudioHardwareCreateProcessTap(description, &tapID), "create tap")

        guard let tapUID: CFString = Self.get(tapID, kAudioTapPropertyUID) else { throw Failure(step: "tap uid", code: -1) }
        if let format: AudioStreamBasicDescription = Self.get(tapID, kAudioTapPropertyFormat) {
            sampleRate = format.mSampleRate
        }
        let output: AudioObjectID = Self.get(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultSystemOutputDevice) ?? 0
        guard let outputUID: CFString = Self.get(output, kAudioDevicePropertyDeviceUID) else { throw Failure(step: "output uid", code: -1) }

        // As in Apple's "Capturing system audio with Core Audio taps": a private aggregate device
        // clocked by the output device, with the tap as its input.
        let aggregate: [String: Any] = [
            kAudioAggregateDeviceNameKey: "Nightjar YouTube Tap",
            kAudioAggregateDeviceUIDKey: UUID().uuidString,
            kAudioAggregateDeviceMainSubDeviceKey: outputUID as String,
            kAudioAggregateDeviceIsPrivateKey: true,
            kAudioAggregateDeviceIsStackedKey: false,
            kAudioAggregateDeviceTapAutoStartKey: true,
            kAudioAggregateDeviceSubDeviceListKey: [[kAudioSubDeviceUIDKey: outputUID as String]],
            kAudioAggregateDeviceTapListKey: [[kAudioSubTapDriftCompensationKey: true, kAudioSubTapUIDKey: tapUID as String]],
        ]
        try check(AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &aggregateID), "create aggregate device")
        try check(AudioDeviceCreateIOProcIDWithBlock(&procID, aggregateID, nil) { [weak self] _, input, _, _, _ in
            self?.capture(input)
        }, "create io proc")
        try check(AudioDeviceStart(aggregateID, procID), "start capture")
    }

    func stop() {
        if aggregateID != kAudioObjectUnknown {
            if let procID {
                AudioDeviceStop(aggregateID, procID)
                AudioDeviceDestroyIOProcID(aggregateID, procID)
            }
            AudioHardwareDestroyAggregateDevice(aggregateID)
        }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        procID = nil
        aggregateID = AudioObjectID(kAudioObjectUnknown)
        tapID = AudioObjectID(kAudioObjectUnknown)
        tapped = []
    }

    deinit {
        stop()
        vDSP_destroy_fftsetup(fft)
    }

    // MARK: Audio thread

    /// Float32 buffers from the tap (stereo mixdown): mixed to mono into the ring.
    private func capture(_ input: UnsafePointer<AudioBufferList>) {
        let list = UnsafeMutableAudioBufferListPointer(UnsafeMutablePointer(mutating: input))
        for buffer in list {
            guard let data = buffer.mData?.assumingMemoryBound(to: Float.self) else { continue }
            let channels = max(1, Int(buffer.mNumberChannels))
            let frames = Int(buffer.mDataByteSize) / 4 / channels
            lock.withLockUnchecked {
                let mask = ring.count - 1
                for f in 0..<frames {
                    var sum: Float = 0
                    for c in 0..<channels { sum += data[f * channels + c] }
                    ring[head & mask] = sum / Float(channels)
                    head &+= 1
                }
            }
        }
    }

    // MARK: Analysis

    /// The latest bins (0...1) and waveform, or nil when no new audio arrived since the last call.
    func analyse() -> (bins: [Double], wave: [Double])? {
        var samples = [Float](repeating: 0, count: Self.window)
        let fresh: Bool = lock.withLockUnchecked {
            guard head != lastRead, head >= Self.window else { return false }
            lastRead = head
            let mask = ring.count - 1
            for k in 0..<Self.window { samples[k] = ring[(head - Self.window + k) & mask] }
            return true
        }
        guard fresh else { return nil }

        let half = Self.window / 2
        var windowed = [Float](repeating: 0, count: Self.window)
        vDSP_vmul(samples, 1, hann, 1, &windowed, 1, vDSP_Length(Self.window))
        var re = [Float](repeating: 0, count: half), im = [Float](repeating: 0, count: half)
        var magnitudes = [Float](repeating: 0, count: half)
        re.withUnsafeMutableBufferPointer { rp in
            im.withUnsafeMutableBufferPointer { ip in
                var split = DSPSplitComplex(realp: rp.baseAddress!, imagp: ip.baseAddress!)
                windowed.withUnsafeBufferPointer { wp in
                    wp.baseAddress!.withMemoryRebound(to: DSPComplex.self, capacity: half) {
                        vDSP_ctoz($0, 2, &split, 1, vDSP_Length(half))
                    }
                }
                vDSP_fft_zrip(fft, &split, 1, vDSP_Length(log2(Double(Self.window))), FFTDirection(FFT_FORWARD))
                vDSP_zvabs(&split, 1, &magnitudes, 1, vDSP_Length(half))
            }
        }
        // zrip output is twice the plain DFT, and a full-scale sine through a Hann window peaks at
        // N/4: amplitude = |X| * 2 / N.
        let norm = 2.0 / Double(Self.window)
        let hzPerBin = sampleRate / Double(Self.window)
        var bins = [Double](repeating: 0, count: Self.bins)
        for i in 0..<Self.bins {
            let lo = Self.loHz * pow(Self.hiHz / Self.loHz, Double(i) / Double(Self.bins))
            let hi = Self.loHz * pow(Self.hiHz / Self.loHz, Double(i + 1) / Double(Self.bins))
            let a = max(1, Int(lo / hzPerBin))
            let b = min(half, max(a + 1, Int((hi / hzPerBin).rounded(.up))))
            let peak = Double(magnitudes[a..<b].max() ?? 0) * norm
            let db = 20 * log10(max(peak, 1e-9))
            bins[i] = ((min(1, max(0, (db - Self.floorDB) / (Self.ceilDB - Self.floorDB)))) * 100).rounded() / 100
        }
        let step = Self.window / Self.wave
        let wave = (0..<Self.wave).map { (Double(samples[$0 * step]) * 1000).rounded() / 1000 }
        return (bins, wave)
    }

    // MARK: Core Audio properties

    private static func address(_ selector: AudioObjectPropertySelector) -> AudioObjectPropertyAddress {
        AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
    }

    static func get<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> T? {
        var addr = address(selector)
        var size = UInt32(MemoryLayout<T>.size)
        let value = UnsafeMutablePointer<T>.allocate(capacity: 1)
        defer { value.deallocate() }
        guard AudioObjectGetPropertyData(object, &addr, 0, nil, &size, value) == noErr else { return nil }
        return value.pointee
    }

    static func getArray<T>(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> [T] {
        var addr = address(selector)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(object, &addr, 0, nil, &size) == noErr, size > 0 else { return [] }
        return [T](unsafeUninitializedCapacity: Int(size) / MemoryLayout<T>.size) { buf, count in
            var s = size
            count = AudioObjectGetPropertyData(object, &addr, 0, nil, &s, buf.baseAddress!) == noErr ? Int(s) / MemoryLayout<T>.size : 0
        }
    }
}
