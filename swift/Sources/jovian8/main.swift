import Foundation
import Jovian8Core

// Command line companion: render presets offline, inspect and convert Jup-8 V preset files.
let usage = """
usage:
  jovian8 info <preset|.jup4x>                     show a preset's header and modelled values
  jovian8 render <preset|.jup4x> <out.wav> [script]   render with the Jovian-8 engine
        script: lines "on <note> <vel> <sec>", "off <note> <sec>", "cc <n> <v> <sec>", "bend <v14> <sec>",
                "len <sec>", "set <ParamID>=<normalized>"; default plays C4 for 1 s
  jovian8 export <out.jup4x> <preset|.jup4x>...     bundle presets into a Jup-8 V import file
  jovian8 export-builtin <out.jup4x>                 export the built-in Jovian-8 patches
  jovian8 roundtrip [folder]                         check parse -> state -> preset -> parse on a library
  jovian8 list                                       list presets found by the plugin browser
"""

func fail(_ s: String) -> Never { FileHandle.standardError.write((s + "\n").data(using: .utf8)!); exit(1) }

func loadStates(_ path: String) -> [PatchState] {
    guard let data = FileManager.default.contents(atPath: path) else { fail("cannot read \(path)") }
    do { return try Jup4xArchive.read(data).map(PatchState.init(preset:)) } catch { fail("\(path): \(error)") }
}

func writeWav(_ url: URL, _ l: [Float], _ r: [Float], sampleRate: Int) throws {
    var d = Data()
    func u32(_ v: UInt32) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
    func u16(_ v: UInt16) { withUnsafeBytes(of: v.littleEndian) { d.append(contentsOf: $0) } }
    let bytes = UInt32(l.count * 8)
    d.append(contentsOf: Array("RIFF".utf8)); u32(36 + bytes); d.append(contentsOf: Array("WAVEfmt ".utf8))
    u32(16); u16(3); u16(2); u32(UInt32(sampleRate)); u32(UInt32(sampleRate * 8)); u16(8); u16(32)
    d.append(contentsOf: Array("data".utf8)); u32(bytes)
    for i in 0..<l.count {
        withUnsafeBytes(of: l[i]) { d.append(contentsOf: $0) }
        withUnsafeBytes(of: r[i]) { d.append(contentsOf: $0) }
    }
    try d.write(to: url)
}

let args = Array(CommandLine.arguments.dropFirst())
guard let cmd = args.first else { print(usage); exit(0) }

switch cmd {
case "info":
    guard args.count >= 2, let data = FileManager.default.contents(atPath: args[1]) else { fail(usage) }
    do {
        for p in try Jup4xArchive.read(data) {
            print("\(p.name)  [bank \(p.pack)]  by \(p.author)  v\(p.version)  \(p.params.count) params, \(p.blobs.count) blobs")
            if !p.comment.isEmpty { print("  \(p.comment)") }
            for (k, v) in p.meta.sorted(by: { $0.key < $1.key }) { print("  meta \(k) = \(v)") }
            let st = PatchState(preset: p)
            for s in ParamTable.all where s.group.rawValue.hasPrefix("fx") == false && s.group != .matrix && s.group != .keyboard {
                let v = st.values[s.id] ?? s.defaultValue
                print(String(format: "  %-22@ %-24@ %@", s.id as NSString, s.name as NSString, s.displayString(v) as NSString))
            }
        }
    } catch { fail("\(error)") }

case "render":
    guard args.count >= 3 else { fail(usage) }
    let state = loadStates(args[1]).first ?? PatchState()
    let engine = SynthEngine()
    let sr = 48_000
    engine.prepare(sampleRate: Double(sr), maxFrames: 512)
    state.apply(to: engine.params)
    var script = "on 60 100 0.0\noff 60 1.0\nlen 2.5"
    if args.count >= 4, let s = try? String(contentsOfFile: args[3], encoding: .utf8) { script = s }
    struct Ev { let frame: Int; let bytes: (UInt8, UInt8, UInt8) }
    var events = [Ev]()
    var length = 2.5
    for raw in script.split(separator: "\n") {
        let f = raw.split(separator: " ").map(String.init)
        guard let c = f.first else { continue }
        func t(_ i: Int) -> Int { Int((Double(f[i]) ?? 0) * Double(sr)) }
        switch c {
        case "on": events.append(Ev(frame: t(3), bytes: (0x90, UInt8(f[1])!, UInt8(f[2])!)))
        case "off": events.append(Ev(frame: t(2), bytes: (0x80, UInt8(f[1])!, 0)))
        case "cc": events.append(Ev(frame: t(3), bytes: (0xB0, UInt8(f[1])!, UInt8(f[2])!)))
        case "bend": let v = Int(f[1])!; events.append(Ev(frame: t(2), bytes: (0xE0, UInt8(v & 0x7F), UInt8(v >> 7 & 0x7F))))
        case "len": length = Double(f[1]) ?? length
        case "set":
            let kv = f[1].split(separator: "=")
            if kv.count == 2, ParamTable.addressByID[String(kv[0])] != nil { engine.params[String(kv[0])] = Float(kv[1]) ?? 0 }
        default: break
        }
    }
    events.sort { $0.frame < $1.frame }
    let total = Int(length * Double(sr))
    var outL = [Float](repeating: 0, count: total), outR = outL
    let block = 256
    var ei = 0, pos = 0
    let bl = UnsafeMutablePointer<Float>.allocate(capacity: block), br = UnsafeMutablePointer<Float>.allocate(capacity: block)
    let start = Date()
    while pos < total {
        var n = min(block, total - pos)
        if ei < events.count, events[ei].frame <= pos {
            engine.handleMIDI(events[ei].bytes.0, events[ei].bytes.1, events[ei].bytes.2); ei += 1; continue
        }
        if ei < events.count { n = min(n, events[ei].frame - pos) }
        engine.render(frames: n, left: bl, right: br, time: HostTime(tempo: 120, beatPosition: Double(pos) / Double(sr) * 2, isPlaying: true))
        for i in 0..<n { outL[pos + i] = bl[i]; outR[pos + i] = br[i] }
        pos += n
    }
    let secs = Date().timeIntervalSince(start)
    try writeWav(URL(fileURLWithPath: args[2]), outL, outR, sampleRate: sr)
    let peak = outL.map(abs).max() ?? 0
    print(String(format: "rendered %@ -> %@  peak %.4f  realtime x%.1f", state.name, args[2], peak, length / max(secs, 1e-6)))

case "export":
    guard args.count >= 3 else { fail(usage) }
    let states = args.dropFirst(2).flatMap(loadStates)
    guard !states.isEmpty else { fail("no presets to export") }
    let url = URL(fileURLWithPath: args[1])
    try Jup4xArchive.make(states.map { $0.arturiaPreset() }).write(to: url)
    print("wrote \(states.count) presets to \(url.path)")

case "export-builtin":
    guard args.count >= 2 else { fail(usage) }
    try Jup4xArchive.make(FactoryPatches.all.map { $0.arturiaPreset() }).write(to: URL(fileURLWithPath: args[1]))
    print("wrote \(FactoryPatches.all.count) presets to \(args[1])")

case "roundtrip":
    let root = args.count >= 2 ? URL(fileURLWithPath: args[1]) : PresetLocations.arturiaRoot
    var ok = 0, failed = 0, maxErr = 0.0, identical = 0
    let e = FileManager.default.enumerator(at: root, includingPropertiesForKeys: nil)
    while let url = e?.nextObject() as? URL {
        guard url.pathExtension.isEmpty, let data = try? Data(contentsOf: url), ArturiaPreset.looksLikePreset(data) else { continue }
        do {
            let original = try ArturiaPreset.parse(data)
            if original.serialized() == data { identical += 1 }
            let state = PatchState(preset: original)
            let written = state.arturiaPreset().serialized()
            let reread = try ArturiaPreset.parse(written)
            // every original parameter must survive with float precision
            for (k, v) in original.params {
                guard let w = reread.params[k] else { throw NSError(domain: "missing \(k)", code: 1) }
                let expected = ParamTable.addressByID[k] != nil ? (original.normalizedForCurrentVersion(k) ?? v) : v
                maxErr = max(maxErr, abs(Double(Float(expected)) - w))
            }
            guard reread.name == original.name, reread.meta == original.meta || !original.meta.isEmpty else { throw NSError(domain: "header", code: 2) }
            let state2 = PatchState(preset: reread)
            guard state2.values == state.values else { throw NSError(domain: "values differ", code: 3) }
            ok += 1
        } catch {
            failed += 1
            print("FAIL \(url.path): \(error)")
        }
    }
    print("round trip: \(ok) ok, \(failed) failed, max value error \(maxErr); re-serialized byte-identical: \(identical)")

case "list":
    for e in PresetLibrary.scan() { print("\(e.source.rawValue) / \(e.bank) / \(e.name)") }

default:
    print(usage)
}
