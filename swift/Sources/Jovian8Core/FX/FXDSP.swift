import Foundation

// Building blocks shared by the effects. Everything is allocation-free once prepared.

/// Samples per processing chunk: smoothed parameters and filter coefficients update at this rate.
let fxChunk = 32

/// Zeroes values small enough to decay into denormals (used on every recursive path).
@inline(__always) func fxFlush(_ x: Float) -> Float { abs(x) < 1e-15 ? 0 : x }

@inline(__always) func fxDbToGain(_ db: Float) -> Float { expf(db * 0.115_129_25) }

/// One-pole smoothing coefficient for time constant `tau` (s), advanced every `step` samples.
func fxSmoothCoef(_ tau: Double, _ sampleRate: Double, step: Int = 1) -> Float {
    Float(1 - exp(-Double(step) / (tau * sampleRate)))
}

/// Cubic soft clip: unity slope at 0, reaches ±limit with zero slope at ±1.5·limit.
@inline(__always) func fxSoftClip(_ x: Float, _ limit: Float) -> Float {
    let u = x / (1.5 * limit)
    if u >= 1 { return limit }
    if u <= -1 { return -limit }
    return 1.5 * limit * (u - u * u * u * (1.0 / 3.0))
}

/// sin(2π·p) for p in [0, 1); max error about 0.001 (parabola with one refinement step).
@inline(__always) func fxSine(_ p: Float) -> Float {
    let t = p < 0.5 ? p : p - 1
    let y = 8 * t * (1 - 2 * abs(t))
    return y * (0.775 + 0.225 * abs(y))
}

/// Triangle in phase with `fxSine` (0 at p = 0, +1 at p = 0.25).
@inline(__always) func fxTriangle(_ p: Float) -> Float {
    let t = p < 0.5 ? p : p - 1
    let a = abs(t)
    let v = a <= 0.25 ? 4 * a : 2 - 4 * a
    return t < 0 ? -v : v
}

/// Wraps a phase that is at most one cycle out of [0, 1).
@inline(__always) func fxWrap(_ p: Float) -> Float { p >= 1 ? p - 1 : (p < 0 ? p + 1 : p) }

/// TPT one-pole coefficient G = g / (1 + g) for a cutoff in Hz.
@inline(__always) func fxOnePoleG(_ hz: Float, _ sampleRate: Float) -> Float {
    let g = tanf(Float.pi * min(max(hz, 1), 0.47 * sampleRate) / sampleRate)
    return g / (1 + g)
}

/// Zero-delay-feedback one-pole filter (lowpass, highpass and allpass outputs).
struct FXOnePole {
    var s: Float = 0
    @inline(__always) mutating func lp(_ x: Float, _ G: Float) -> Float {
        let v = (x - s) * G
        let y = v + s
        s = y + v
        return y
    }
    @inline(__always) mutating func hp(_ x: Float, _ G: Float) -> Float { x - lp(x, G) }
    @inline(__always) mutating func ap(_ x: Float, _ G: Float) -> Float { 2 * lp(x, G) - x }
    mutating func flush() { s = fxFlush(s) }
}

/// Coefficients of a trapezoidal state-variable filter (A. Simper, "SvfLinearTrapOptimised2").
/// Output = m0·input + m1·band + m2·low; the shelf/bell forms match the RBJ cookbook responses.
struct FXSVFCoef {
    var a1: Float = 1, a2: Float = 0, a3: Float = 0
    var m0: Float = 1, m1: Float = 0, m2: Float = 0

    @inline(__always) static func g(_ hz: Float, _ sampleRate: Float) -> Float {
        tanf(Float.pi * min(max(hz, 5), 0.47 * sampleRate) / sampleRate)
    }

    private mutating func set(_ g: Float, _ k: Float) {
        a1 = 1 / (1 + g * (g + k)); a2 = g * a1; a3 = g * a2
    }

    static func lowpass(_ g: Float, q: Float) -> FXSVFCoef {
        var c = FXSVFCoef(); c.set(g, 1 / q); c.m0 = 0; c.m1 = 0; c.m2 = 1; return c
    }
    static func highpass(_ g: Float, q: Float) -> FXSVFCoef {
        var c = FXSVFCoef(); let k = 1 / q; c.set(g, k); c.m0 = 1; c.m1 = -k; c.m2 = -1; return c
    }
    /// Bandpass normalized to 0 dB at the centre frequency.
    static func bandpass(_ g: Float, q: Float) -> FXSVFCoef {
        var c = FXSVFCoef(); let k = 1 / q; c.set(g, k); c.m0 = 0; c.m1 = k; c.m2 = 0; return c
    }
    static func bell(_ g: Float, q: Float, db: Float) -> FXSVFCoef {
        var c = FXSVFCoef(); let A = powf(10, db / 40); let k = 1 / (q * A)
        c.set(g, k); c.m0 = 1; c.m1 = k * (A * A - 1); c.m2 = 0; return c
    }
    static func lowShelf(_ g: Float, q: Float, db: Float) -> FXSVFCoef {
        var c = FXSVFCoef(); let A = powf(10, db / 40); let k = 1 / q
        c.set(g / A.squareRoot(), k); c.m0 = 1; c.m1 = k * (A - 1); c.m2 = A * A - 1; return c
    }
    static func highShelf(_ g: Float, q: Float, db: Float) -> FXSVFCoef {
        var c = FXSVFCoef(); let A = powf(10, db / 40); let k = 1 / q
        c.set(g * A.squareRoot(), k); c.m0 = A * A; c.m1 = k * (1 - A) * A; c.m2 = 1 - A * A; return c
    }
}

struct FXSVFState {
    var ic1: Float = 0, ic2: Float = 0
    @inline(__always) mutating func tick(_ v0: Float, _ c: FXSVFCoef) -> Float {
        let v3 = v0 - ic2
        let v1 = c.a1 * ic1 + c.a2 * v3
        let v2 = ic2 + c.a2 * ic1 + c.a3 * v3
        ic1 = 2 * v1 - ic1
        ic2 = 2 * v2 - ic2
        return c.m0 * v0 + c.m1 * v1 + c.m2 * v2
    }
    mutating func flush() { ic1 = fxFlush(ic1); ic2 = fxFlush(ic2) }
}

/// Power-of-two circular buffer. After `write`, `at(0)` is the newest sample and `at(d)` the
/// sample written d calls earlier. The owner calls `allocate` off the render thread and
/// `deallocate` in its deinit.
struct FXDelayLine {
    private(set) var buf: UnsafeMutablePointer<Float>
    private(set) var mask = 0
    private var w = 0

    init() {
        buf = .allocate(capacity: 1)
        buf.initialize(to: 0)
    }

    /// Reallocates for at least `length` samples of history.
    mutating func allocate(_ length: Int) {
        buf.deallocate()
        var size = 4
        while size < length + 4 { size <<= 1 }
        buf = .allocate(capacity: size)
        buf.initialize(repeating: 0, count: size)
        mask = size - 1
        w = 0
    }

    func deallocate() { buf.deallocate() }

    mutating func clear() {
        buf.update(repeating: 0, count: mask + 1)
        w = 0
    }

    @inline(__always) mutating func write(_ x: Float) {
        w = (w + 1) & mask
        buf[w] = x
    }

    @inline(__always) func at(_ d: Int) -> Float { buf[(w - d) & mask] }

    /// Linearly interpolated read, d >= 0.
    @inline(__always) func linear(_ d: Float) -> Float {
        let i = Int(d)
        let f = d - Float(i)
        let a = buf[(w - i) & mask]
        let b = buf[(w - i - 1) & mask]
        return a + (b - a) * f
    }
}

/// Schroeder allpass of `d` samples (d >= 1): v = x + g·v[n-d], y = v[n-d] - g·v.
@inline(__always) func fxAllpass(_ line: inout FXDelayLine, _ x: Float, _ d: Int, _ g: Float) -> Float {
    let z = line.at(d - 1)
    let v = x + g * z
    line.write(fxFlush(v))
    return z - g * v
}

/// Allpass with a fractional (modulated) length, d >= 1.
@inline(__always) func fxAllpass(_ line: inout FXDelayLine, _ x: Float, frac d: Float, _ g: Float) -> Float {
    let z = line.linear(d - 1)
    let v = x + g * z
    line.write(fxFlush(v))
    return z - g * v
}

/// xorshift32 noise for sample & hold LFOs.
struct FXRandom {
    var s: UInt32 = 0x9E37_79B9
    @inline(__always) mutating func bipolar() -> Float {
        s ^= s << 13; s ^= s >> 17; s ^= s << 5
        return Float(Int32(bitPattern: s)) * (1.0 / 2_147_483_648.0)
    }
}

/// Physical mapping of one parameter, copied out of `ParamTable` at init so the render thread
/// never touches reference-counted data.
struct FXParam {
    let addr: Int
    let curve: Curve
    let lo: Double, hi: Double
    let count: Int
    let reverse: Bool

    init(_ id: String) {
        let s = ParamTable.spec(id)
        addr = s.address
        switch s.kind {
        case .continuous(let c, let lo, let hi, _):
            curve = c; self.lo = lo; self.hi = hi; count = 0
            if case .reverse = c { reverse = true } else { reverse = false }
        case .discrete(let items):
            curve = .linear; lo = 0; hi = Double(max(items.count - 1, 0)); count = items.count; reverse = false
        }
    }

    init(slot: Int, _ name: String) { self.init("FX\(slot + 1)_\(name)") }

    /// Normalized value plus a modulation offset, clamped to 0...1.
    @inline(__always) func norm(_ p: UnsafePointer<Float>, _ mod: Float = 0) -> Double {
        let x = Double(p[addr] + mod)
        return x.isNaN ? 0 : min(max(x, 0), 1)
    }

    @inline(__always) func value(_ p: UnsafePointer<Float>, _ mod: Float = 0) -> Double {
        let x = norm(p, mod)
        if reverse { return hi - (hi - lo) * x }
        return lo + (hi - lo) * curve.shape(x)
    }

    @inline(__always) func float(_ p: UnsafePointer<Float>, _ mod: Float = 0) -> Float { Float(value(p, mod)) }

    @inline(__always) func index(_ p: UnsafePointer<Float>, _ mod: Float = 0) -> Int {
        ParamSpec.index(norm(p, mod), count: count)
    }
}

/// Note values of a synced-rate item list in bars of 4/4 ("1/16" -> 0.0625).
func fxBars(_ id: String) -> [Double] {
    ParamTable.spec(id).items.map { s in
        let parts = s.split(separator: "/")
        if parts.count == 2, let a = Double(parts[0]), let b = Double(parts[1]) { return a / b }
        return Double(s) ?? 1
    }
}

/// Duration of `bars` at `bpm`; `type` 0 binary, 1 triplet (×2/3), 2 dotted (×1.5).
@inline(__always) func fxSyncSeconds(_ bars: Double, _ type: Int, _ bpm: Double) -> Double {
    let t = type == 1 ? 2.0 / 3.0 : (type == 2 ? 1.5 : 1.0)
    return bars * t * 240 / bpm
}
