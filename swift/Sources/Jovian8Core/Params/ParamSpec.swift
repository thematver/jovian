import Foundation

/// Response curve from a normalized 0...1 value to the physical range, matching the mapping
/// names used by Jup-8 V (`X(n)`, `Exp(k)`, `BipolarExp(k)`, `reverse`). Each curve was checked
/// against the display strings of the original plugin.
public enum Curve: Sendable, Equatable {
    case linear
    /// `X(n)`: x^n
    case power(Double)
    /// `Exp(k)`: (e^(kx) - 1) / (e^k - 1)
    case exp(Double)
    /// `BipolarExp(k)`: two mirrored `Exp(|k|)` halves around 0.5. Negative k is steepest in the
    /// middle, positive k is flattest in the middle (fine control around zero for bipolar amounts).
    case bipolarExp(Double)
    /// `reverse`: linear, from max down to min.
    case reverse

    @inline(__always)
    static func expShape(_ u: Double, _ k: Double) -> Double {
        abs(k) < 1e-9 ? u : (Foundation.exp(k * u) - 1) / (Foundation.exp(k) - 1)
    }

    @inline(__always)
    static func expShapeInverse(_ y: Double, _ k: Double) -> Double {
        abs(k) < 1e-9 ? y : Foundation.log(1 + y * (Foundation.exp(k) - 1)) / k
    }

    /// Shapes a normalized value; the result is still 0...1.
    @inline(__always)
    public func shape(_ x: Double) -> Double {
        switch self {
        case .linear, .reverse: return x
        case .power(let n): return Foundation.pow(max(0, x), n)
        case .exp(let k): return Curve.expShape(x, k)
        case .bipolarExp(let k):
            let d = 2 * x - 1
            let s: Double = d < 0 ? -1 : 1
            if k >= 0 { return 0.5 + s * 0.5 * Curve.expShape(abs(d), k) }
            return 0.5 + s * 0.5 * (1 - Curve.expShape(1 - abs(d), -k))
        }
    }

    public func unshape(_ y: Double) -> Double {
        let y = min(max(y, 0), 1)
        switch self {
        case .linear, .reverse: return y
        case .power(let n): return Foundation.pow(y, 1 / n)
        case .exp(let k): return Curve.expShapeInverse(y, k)
        case .bipolarExp(let k):
            let d = 2 * y - 1
            let s: Double = d < 0 ? -1 : 1
            if k >= 0 { return 0.5 + s * 0.5 * Curve.expShapeInverse(abs(d), k) }
            return 0.5 + s * 0.5 * (1 - Curve.expShapeInverse(1 - abs(d), -k))
        }
    }
}

public enum ParamUnit: String, Sendable {
    case none, hz, ms, seconds, semitones, percent, decibels, degrees, bits, ratio, bpm
}

public enum ParamGroup: String, Sendable, CaseIterable {
    case lfo, vco, vcomod, vcf, vca, env1, env2, master, voice, dispersion, arp, bender, modwheel
    case lfo2, lfo3, matrix, keyboard, fx, fx1, fx2, fx3

    public var title: String {
        switch self {
        case .lfo: return "LFO"
        case .vco: return "VCO"
        case .vcomod: return "VCO Modulator"
        case .vcf: return "VCF"
        case .vca: return "VCA"
        case .env1: return "Envelope 1"
        case .env2: return "Envelope 2"
        case .master: return "Master"
        case .voice: return "Voice"
        case .dispersion: return "Dispersion"
        case .arp: return "Arpeggio"
        case .bender: return "Bender"
        case .modwheel: return "Mod Wheel"
        case .lfo2: return "LFO 2"
        case .lfo3: return "LFO 3"
        case .matrix: return "Mod Matrix"
        case .keyboard: return "Keyboard Curves"
        case .fx: return "Effects"
        case .fx1: return "FX 1"
        case .fx2: return "FX 2"
        case .fx3: return "FX 3"
        }
    }
}

public enum ParamKind: Sendable {
    case continuous(Curve, min: Double, max: Double, unit: ParamUnit)
    case discrete([String])
}

/// One plugin parameter. `id` is the Jup-8 V preset key, so values map 1:1 between the formats.
/// All values are stored normalized (0...1) exactly as in a Jup-8 V preset; discrete values are
/// stored as `index / (count - 1)` and read back as `min(floor(x * count), count - 1)`.
public struct ParamSpec: Sendable {
    public let id: String
    public let name: String
    public let group: ParamGroup
    public let defaultValue: Double
    public let kind: ParamKind
    public internal(set) var address: Int = 0

    init(id: String, name: String, group: ParamGroup, defaultValue: Double, _ kind: ParamKind) {
        self.id = id
        self.name = name
        self.group = group
        self.defaultValue = defaultValue
        self.kind = kind
    }

    public var isDiscrete: Bool {
        if case .discrete = kind { return true }
        return false
    }

    public var items: [String] {
        if case .discrete(let items) = kind { return items }
        return []
    }

    public var itemCount: Int { items.count }

    /// Physical value of a normalized continuous parameter (or the index of a discrete one).
    @inline(__always)
    public func physical(_ x: Double) -> Double {
        switch kind {
        case .continuous(let c, let lo, let hi, _):
            if case .reverse = c { return hi - (hi - lo) * x }
            return lo + (hi - lo) * c.shape(x)
        case .discrete(let items):
            return Double(ParamSpec.index(x, count: items.count))
        }
    }

    public func normalized(fromPhysical v: Double) -> Double {
        switch kind {
        case .continuous(let c, let lo, let hi, _):
            guard hi != lo else { return 0 }
            if case .reverse = c { return min(max((hi - v) / (hi - lo), 0), 1) }
            return c.unshape((v - lo) / (hi - lo))
        case .discrete(let items):
            return ParamSpec.normalized(index: Int(v.rounded()), count: items.count)
        }
    }

    @inline(__always)
    public static func index(_ x: Double, count: Int) -> Int {
        guard count > 1 else { return 0 }
        return min(max(Int((x * Double(count)).rounded(.down)), 0), count - 1)
    }

    @inline(__always)
    public static func normalized(index: Int, count: Int) -> Double {
        guard count > 1 else { return 0 }
        return Double(min(max(index, 0), count - 1)) / Double(count - 1)
    }

    public func displayString(_ x: Double) -> String {
        if let special = ParamDisplay.special(id: id, normalized: x) { return special }
        switch kind {
        case .discrete(let items):
            return items[ParamSpec.index(x, count: items.count)]
        case .continuous(_, let lo, let hi, let unit):
            return ParamDisplay.format(physical(x), unit: unit, unitless01: lo == 0 && hi == 1)
        }
    }
}

enum ParamDisplay {
    static func format(_ v: Double, unit: ParamUnit, unitless01: Bool) -> String {
        switch unit {
        case .hz:
            return v >= 1000 ? String(format: "%.2f kHz", v / 1000) : String(format: v < 10 ? "%.2f Hz" : "%.1f Hz", v)
        case .ms:
            return v >= 1000 ? String(format: "%.2f s", v / 1000) : String(format: v < 10 ? "%.2f ms" : "%.0f ms", v)
        case .seconds:
            return v >= 1 ? String(format: "%.2f s", v) : String(format: "%.0f ms", v * 1000)
        case .semitones: return String(format: "%.2f st", v)
        case .percent: return String(format: "%.0f %%", v)
        case .decibels: return String(format: "%.1f dB", v)
        case .degrees: return String(format: "%.0f°", v)
        case .bits: return String(format: "%.1f bits", v)
        case .ratio: return String(format: "%.1fx", v)
        case .bpm: return String(format: "%.0f BPM", v)
        case .none: return unitless01 ? String(format: "%.1f", v * 10) : String(format: "%.2f", v)
        }
    }

    static func special(id: String, normalized x: Double) -> String? {
        switch id {
        case "LpfCut":
            return format(VoiceMath.cutoffHz(x), unit: .hz, unitless01: false)
        case "VCA_EnvAmount", "LpfRes", "OscCrossMod", "OscMix", "LpfEnvAmount", "LfoFiltAmount",
             "LPF_KeyFollowAmount", "VCA_LfoAmount", "PanSpread", "BendVCFAmount", "LfoModVCOAmount", "LfoModVcfAmount":
            return String(format: "%.1f", x * 10)
        default:
            return nil
        }
    }
}

/// The parameter table: lookup by address (array index) or by Jup-8 V id.
public enum ParamTable {
    public static let all: [ParamSpec] = {
        var specs = generated
        for i in specs.indices { specs[i].address = i }
        return specs
    }()

    public static let addressByID: [String: Int] = {
        var d = [String: Int]()
        for s in all { d[s.id] = s.address }
        return d
    }()

    public static var count: Int { all.count }

    public static func address(_ id: String) -> Int {
        guard let a = addressByID[id] else { preconditionFailure("unknown parameter \(id)") }
        return a
    }

    public static func spec(_ id: String) -> ParamSpec { all[address(id)] }
}

/// Plain-data copy of a parameter's mapping for the render thread (no reference-counted fields).
public struct FastParam: Sendable {
    public let curve: Curve
    public let lo: Double
    public let hi: Double
    /// Item count for discrete parameters, 0 for continuous ones.
    public let count: Int

    @inline(__always)
    public func physical(_ x: Float) -> Double {
        let x = Double(x)
        if count > 0 { return Double(ParamSpec.index(x, count: count)) }
        if case .reverse = curve { return hi - (hi - lo) * x }
        return lo + (hi - lo) * curve.shape(x)
    }

    @inline(__always)
    public func index(_ x: Float) -> Int { ParamSpec.index(Double(x), count: count) }
}

extension ParamTable {
    /// Render-thread lookup table, indexed by address.
    public static let fast: UnsafeMutablePointer<FastParam> = {
        let p = UnsafeMutablePointer<FastParam>.allocate(capacity: all.count)
        for s in all {
            switch s.kind {
            case .continuous(let c, let lo, let hi, _):
                (p + s.address).initialize(to: FastParam(curve: c, lo: lo, hi: hi, count: 0))
            case .discrete(let items):
                (p + s.address).initialize(to: FastParam(curve: .linear, lo: 0, hi: Double(max(items.count - 1, 0)), count: items.count))
            }
        }
        return p
    }()
}

/// Lock-free storage of normalized parameter values shared between the UI/host thread (writer)
/// and the render thread (reader). Aligned 32-bit stores are atomic on the supported CPUs.
public final class ParamStore: @unchecked Sendable {
    public let values: UnsafeMutablePointer<Float>
    public let count: Int
    /// Incremented on every write so the engine can cheaply notice changes.
    public private(set) var generation: UInt64 = 0

    public init() {
        count = ParamTable.count
        values = .allocate(capacity: count)
        for s in ParamTable.all { values[s.address] = Float(s.defaultValue) }
    }

    deinit { values.deallocate() }

    @inline(__always)
    public subscript(address: Int) -> Float {
        get { values[address] }
        set { values[address] = newValue; generation &+= 1 }
    }

    public subscript(id: String) -> Float {
        get { values[ParamTable.address(id)] }
        set { self[ParamTable.address(id)] = newValue }
    }

    public func resetToDefaults() {
        for s in ParamTable.all { values[s.address] = Float(s.defaultValue) }
        generation &+= 1
    }
}
