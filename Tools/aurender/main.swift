// Offline render through the installed Jovian-8 Audio Unit (as a host would play it):
//   aurender <out.wav> [note] [seconds-held] [total-seconds]
import AVFoundation
import AudioToolbox

func fourcc(_ s: String) -> FourCharCode { s.utf8.reduce(0) { $0 << 8 | FourCharCode($1) } }
let a = CommandLine.arguments
let out = a.count > 1 ? a[1] : "/tmp/jovian-au.wav"
let note = UInt8(a.count > 2 ? Int(a[2])! : 60)
let held = a.count > 3 ? Double(a[3])! : 1.0
let total = a.count > 4 ? Double(a[4])! : 2.5
let desc = AudioComponentDescription(componentType: kAudioUnitType_MusicDevice, componentSubType: fourcc("JV8e"),
                                     componentManufacturer: fourcc("Jovn"), componentFlags: 0, componentFlagsMask: 0)
let sem = DispatchSemaphore(value: 0)
var unit: AVAudioUnit?
AVAudioUnit.instantiate(with: desc, options: [.loadInProcess]) { u, e in unit = u; if u == nil { print("error \(String(describing: e))") }; sem.signal() }
sem.wait()
guard let inst = unit as? AVAudioUnitMIDIInstrument else { print("not an instrument"); exit(1) }
let engine = AVAudioEngine()
let fmt = AVAudioFormat(standardFormatWithSampleRate: 48000, channels: 2)!
engine.attach(inst)
engine.connect(inst, to: engine.mainMixerNode, format: fmt)
try! engine.enableManualRenderingMode(.offline, format: fmt, maximumFrameCount: 512)
try! engine.start()
let file = try! AVAudioFile(forWriting: URL(fileURLWithPath: out), settings: fmt.settings, commonFormat: .pcmFormatFloat32, interleaved: false)
let buf = AVAudioPCMBuffer(pcmFormat: engine.manualRenderingFormat, frameCapacity: 512)!
var frames = 0, peak: Float = 0, sum: Double = 0
let start = Date()
inst.startNote(note, withVelocity: 100, onChannel: 0)
var off = false
while Double(frames) < total * 48000 {
    if !off && Double(frames) >= held * 48000 { inst.stopNote(note, onChannel: 0); off = true }
    let st = try! engine.renderOffline(512, to: buf)
    guard st == .success else { print("render status \(st.rawValue)"); break }
    for ch in 0..<2 { let p = buf.floatChannelData![ch]; for i in 0..<Int(buf.frameLength) { peak = max(peak, abs(p[i])); sum += Double(p[i] * p[i]) } }
    try! file.write(from: buf)
    frames += Int(buf.frameLength)
}
let rms = 10 * log10(sum / Double(frames * 2) + 1e-30)
print(String(format: "rendered %d frames  peak %.4f  rms %.2f dB  %.0fx realtime", frames, peak, rms, total / Date().timeIntervalSince(start)))
