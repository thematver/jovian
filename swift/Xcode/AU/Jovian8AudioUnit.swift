import AudioToolbox
import AVFoundation
import CoreMIDI
import Foundation
import Jovian8Core

/// The Audio Unit (AUv3 instrument). Parameters are the Jup-8 V preset keys; continuous ones are
/// exposed normalized (0...1, displayed in physical units), discrete ones as indexed menus.
public final class Jovian8AudioUnit: AUAudioUnit, @unchecked Sendable {
    public static let componentDescription = AudioComponentDescription(
        componentType: kAudioUnitType_MusicDevice,
        componentSubType: 0x4A56_3865,          // 'JV8e'
        componentManufacturer: 0x4A6F_766E,     // 'Jovn'
        componentFlags: 0, componentFlagsMask: 0)

    static let stateKey = "jovian8.state"

    public let engine = SynthEngine()
    private var outputBus: AUAudioUnitBus!
    private var busses: AUAudioUnitBusArray!
    private var tree: AUParameterTree!
    private var scratchL: UnsafeMutablePointer<Float>?
    private var scratchR: UnsafeMutablePointer<Float>?
    private var scratchM: UnsafeMutablePointer<Float>?

    /// Name, bank and pass-through data of the current patch (values live in `engine.params`).
    public private(set) var patch = PatchState()
    /// Called on the main thread whenever a whole patch was loaded (preset, state restore).
    public var onPatchLoaded: ((PatchState) -> Void)?

    public override init(componentDescription: AudioComponentDescription, options: AudioComponentInstantiationOptions = []) throws {
        try super.init(componentDescription: componentDescription, options: options)
        let format = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 2)!
        outputBus = try AUAudioUnitBus(format: format)
        outputBus.maximumChannelCount = 2
        busses = AUAudioUnitBusArray(audioUnit: self, busType: .output, busses: [outputBus])
        tree = Self.makeParameterTree()
        let store = engine.params
        tree.implementorValueObserver = { param, value in
            store[Int(param.address)] = Self.normalized(address: Int(param.address), auValue: value)
        }
        tree.implementorValueProvider = { param in
            Self.auValue(address: Int(param.address), normalized: store[Int(param.address)])
        }
        tree.implementorStringFromValueCallback = { param, valuePtr in
            let a = Int(param.address)
            let v = valuePtr?.pointee ?? Self.auValue(address: a, normalized: store[a])
            return ParamTable.all[a].displayString(Double(Self.normalized(address: a, auValue: v)))
        }
        maximumFramesToRender = 4096
        patch.apply(to: store)
    }

    deinit {
        scratchL?.deallocate()
        scratchR?.deallocate()
        scratchM?.deallocate()
    }

    // MARK: Parameters

    static func makeParameterTree() -> AUParameterTree {
        var groups = [AUParameterGroup]()
        for g in ParamGroup.allCases {
            let params = ParamTable.all.filter { $0.group == g }.map { s -> AUParameter in
                let flags: AudioUnitParameterOptions = [.flag_IsReadable, .flag_IsWritable, .flag_CanRamp]
                if s.isDiscrete {
                    return AUParameterTree.createParameter(
                        withIdentifier: s.id, name: s.name, address: AUParameterAddress(s.address),
                        min: 0, max: AUValue(max(s.itemCount - 1, 1)), unit: .indexed, unitName: nil,
                        flags: flags, valueStrings: s.items, dependentParameters: nil)
                }
                let p = AUParameterTree.createParameter(
                    withIdentifier: s.id, name: s.name, address: AUParameterAddress(s.address),
                    min: 0, max: 1, unit: .generic, unitName: nil,
                    flags: flags.union(.flag_ValuesHaveStrings), valueStrings: nil, dependentParameters: nil)
                return p
            }
            groups.append(AUParameterTree.createGroup(withIdentifier: g.rawValue, name: g.title, children: params))
        }
        let t = AUParameterTree.createTree(withChildren: groups)
        for p in t.allParameters { p.value = auValue(address: Int(p.address), normalized: Float(ParamTable.all[Int(p.address)].defaultValue)) }
        return t
    }

    @inline(__always)
    static func normalized(address: Int, auValue: AUValue) -> Float {
        let n = ParamTable.fast[address].count
        guard n > 0 else { return min(max(auValue, 0), 1) }
        return Float(ParamSpec.normalized(index: Int(auValue.rounded()), count: n))
    }

    @inline(__always)
    static func auValue(address: Int, normalized: Float) -> AUValue {
        let f = ParamTable.fast[address]
        return f.count > 0 ? AUValue(f.index(normalized)) : normalized
    }

    public override var parameterTree: AUParameterTree? {
        get { tree }
        set { }
    }

    public override var outputBusses: AUAudioUnitBusArray { busses }
    /// Stereo, or mono (folded down) for mono instrument tracks.
    public override var channelCapabilities: [NSNumber]? { [0, 2, 0, 1] }
    public override var providesUserInterface: Bool { true }
    public override var audioUnitMIDIProtocol: MIDIProtocolID { ._1_0 }

    // MARK: Rendering

    public override func allocateRenderResources() throws {
        try super.allocateRenderResources()
        let frames = Int(maximumFramesToRender)
        scratchL?.deallocate(); scratchR?.deallocate(); scratchM?.deallocate()
        scratchL = .allocate(capacity: frames)
        scratchR = .allocate(capacity: frames)
        scratchM = .allocate(capacity: frames)
        engine.prepare(sampleRate: outputBus.format.sampleRate, maxFrames: frames)
    }

    public override func deallocateRenderResources() {
        super.deallocateRenderResources()
    }

    public override var internalRenderBlock: AUInternalRenderBlock {
        let engine = self.engine
        let store = engine.params
        let unit = self
        return { _, timestamp, frameCount, _, outputData, eventList, _ in
            let abl = UnsafeMutableAudioBufferListPointer(outputData)
            guard abl.count > 0, let sl = unit.scratchL, let sr = unit.scratchR, let sm = unit.scratchM else { return kAudioUnitErr_Uninitialized }
            if abl[0].mData == nil { abl[0].mData = UnsafeMutableRawPointer(abl.count == 1 ? sm : sl) }
            if abl.count > 1, abl[1].mData == nil { abl[1].mData = UnsafeMutableRawPointer(sr) }
            for i in 0..<abl.count { abl[i].mDataByteSize = frameCount * 4 }
            let mono = abl.count == 1
            let left = mono ? sl : abl[0].mData!.assumingMemoryBound(to: Float.self)
            let right = mono ? sr : abl[1].mData!.assumingMemoryBound(to: Float.self)

            var time = HostTime()
            if let mc = unit.musicalContextBlock {
                var tempo = 120.0, beat = 0.0
                if mc(&tempo, nil, nil, &beat, nil, nil) { time.tempo = tempo; time.beatPosition = beat }
            }
            if let ts = unit.transportStateBlock {
                var flags = AUHostTransportStateFlags()
                if ts(&flags, nil, nil, nil) { time.isPlaying = flags.contains(.moving) }
            }
            let sampleRate = engine.sampleRate
            let start = AUEventSampleTime(timestamp.pointee.mSampleTime)
            var event = eventList
            var pos: AUAudioFrameCount = 0
            while pos < frameCount {
                while let e = event {
                    let t = e.pointee.head.eventSampleTime
                    let due = t == AUEventSampleTimeImmediate || t - start <= AUEventSampleTime(pos)
                    guard due else { break }
                    Self.handle(e, engine: engine, store: store)
                    event = UnsafePointer(e.pointee.head.next)
                }
                var end = frameCount
                if let e = event {
                    let rel = e.pointee.head.eventSampleTime - start
                    end = AUAudioFrameCount(min(max(rel, AUEventSampleTime(pos) + 1), AUEventSampleTime(frameCount)))
                }
                var segTime = time
                segTime.beatPosition += Double(pos) / sampleRate * time.tempo / 60
                engine.render(frames: Int(end - pos), left: left + Int(pos), right: right + Int(pos), time: segTime)
                pos = end
            }
            while let e = event {
                Self.handle(e, engine: engine, store: store)
                event = UnsafePointer(e.pointee.head.next)
            }
            if mono {
                let out = abl[0].mData!.assumingMemoryBound(to: Float.self)
                for i in 0..<Int(frameCount) { out[i] = 0.5 * (sl[i] + sr[i]) }
            }
            return noErr
        }
    }

    @inline(__always)
    private static func handle(_ e: UnsafePointer<AURenderEvent>, engine: SynthEngine, store: ParamStore) {
        switch e.pointee.head.eventType {
        case .parameter, .parameterRamp:
            let p = e.pointee.parameter
            let a = Int(p.parameterAddress)
            if a >= 0 && a < store.count { store[a] = normalized(address: a, auValue: p.value) }
        case .MIDI:
            let m = e.pointee.MIDI
            engine.handleMIDI(m.data.0, m.data.1, m.data.2)
        case .midiEventList:
            // MIDI 1.0 channel voice messages wrapped in UMP (message type 2)
            var list = e.pointee.MIDIEventsList.eventList
            withUnsafePointer(to: &list.packet) { first in
                var pkt = first
                for _ in 0..<list.numPackets {
                    let w = pkt.pointee.words.0
                    if (w >> 28) == 0x2 {
                        engine.handleMIDI(UInt8((w >> 16) & 0xFF), UInt8((w >> 8) & 0x7F), UInt8(w & 0x7F))
                    }
                    pkt = UnsafePointer(MIDIEventPacketNext(pkt))
                }
            }
        default:
            break
        }
    }

    // MARK: State & presets

    /// Loads a whole patch (call on the main thread).
    public func load(_ state: PatchState) {
        patch = state
        state.apply(to: engine.params)
        for p in tree.allParameters {
            p.setValue(Self.auValue(address: Int(p.address), normalized: engine.params[Int(p.address)]), originator: nil)
        }
        onPatchLoaded?(state)
    }

    /// The current patch including live parameter values.
    public func currentPatch() -> PatchState {
        var s = patch
        s.capture(from: engine.params)
        return s
    }

    public func rename(name: String, pack: String) {
        patch.name = name
        patch.pack = pack
    }

    public override var fullState: [String: Any]? {
        get {
            var s = super.fullState ?? [:]
            if let data = try? currentPatch().encoded() { s[Self.stateKey] = data }
            return s
        }
        set {
            guard let s = newValue else { return }
            if let data = s[Self.stateKey] as? Data, let st = try? PatchState.decode(data) {
                load(st)
            } else {
                super.fullState = s
            }
        }
    }

    public override var supportsUserPresets: Bool { true }

    public override var factoryPresets: [AUAudioUnitPreset]? {
        FactoryPatches.all.enumerated().map { i, p in
            let preset = AUAudioUnitPreset()
            preset.number = i
            preset.name = p.name
            return preset
        }
    }

    private var _currentPreset: AUAudioUnitPreset?
    public override var currentPreset: AUAudioUnitPreset? {
        get { _currentPreset }
        set {
            guard let preset = newValue else { _currentPreset = nil; return }
            if preset.number >= 0 {
                if preset.number < FactoryPatches.all.count { load(FactoryPatches.all[preset.number]) }
            } else if userPresets.contains(where: { $0.number == preset.number && $0.name == preset.name }),
                      let state = try? presetState(for: preset) {
                // presetState(for:) raises an Objective-C exception for unknown presets, so check first
                fullState = state
            }
            _currentPreset = preset
        }
    }
}
