import Foundation

// MARK: - Fast math

@inline(__always)
func fastExp2(_ x: Float) -> Float {
    // 2^x via integer/fraction split and a 5th order polynomial (rel. error ~2e-7 on [0,1)).
    let xi = x.rounded(.down)
    let f = x - xi
    let p = 1 + f * (0.693_147_18 + f * (0.240_226_51 + f * (0.055_504_11 + f * (0.009_618_13 + f * 0.001_333_36))))
    return p * Float(bitPattern: UInt32(truncatingIfNeeded: (Int32(max(min(xi, 126), -126)) + 127) << 23))
}

@inline(__always)
func fastTanh(_ x: Float) -> Float {
    // Padé approximant, clamped; smooth and monotonic, exact at ±3.
    let c = min(max(x, -3), 3)
    let c2 = c * c
    return c * (27 + c2) / (27 + 9 * c2)
}

@inline(__always)
func fastSin2Pi(_ phase: Float) -> Float {
    // sin(2πφ) for φ in [0, 1): parabolic approximation with one refinement step (error < 0.1 %).
    var x = phase - 0.5          // -0.5...0.5
    x = -x * 2                   // sin(2π(φ)) = -sin(2π(φ-0.5))
    let y = 4 * x * (1 - abs(x))
    return 0.225 * (y * abs(y) - y) + y
}

/// xorshift32 noise in [-1, 1).
struct Noise {
    var state: UInt32 = 0x9E37_79B9
    @inline(__always)
    mutating func next() -> Float {
        state ^= state << 13
        state ^= state >> 17
        state ^= state << 5
        return Float(Int32(bitPattern: state)) * (1.0 / 2_147_483_648.0)
    }
}

// MARK: - PolyBLEP

@inline(__always)
func polyBlep(_ t: Float, _ dt: Float) -> Float {
    if t < dt {
        let x = t / dt
        return x + x - x * x - 1
    } else if t > 1 - dt {
        let x = (t - 1) / dt
        return x * x + x + x + 1
    }
    return 0
}

@inline(__always)
func wrapPhase(_ p: Float) -> Float { p - p.rounded(.down) }

/// Band-limited classic waveforms evaluated at phase `p` with increment `dt` (both in cycles).
enum Wave {
    @inline(__always) static func saw(_ p: Float, _ dt: Float) -> Float {
        2 * p - 1 - polyBlep(p, dt)
    }

    @inline(__always) static func pulse(_ p: Float, _ dt: Float, _ duty: Float) -> Float {
        var v: Float = p < duty ? 1 : -1
        v += polyBlep(p, dt)
        v -= polyBlep(wrapPhase(p - duty + 1), dt)
        return v
    }

    @inline(__always) static func triangle(_ p: Float) -> Float {
        // Oversampled by the engine; the naive triangle's 12 dB/oct harmonics alias negligibly.
        let v = 4 * p - 1
        return v < 1 ? v : 3 - 4 * p
    }
}

// MARK: - Filters

/// One-pole TPT high-pass (the Jupiter's non-resonant 6 dB HPF).
struct OnePoleHP {
    var s: Float = 0
    @inline(__always)
    mutating func process(_ x: Float, g G: Float) -> Float {
        let v = (x - s) * G
        let lp = v + s
        s = lp + v
        return x - lp
    }
}

/// Four-pole OTA cascade with global feedback (IR3109 style), zero-delay feedback solved per
/// sample with a saturating input stage. The 12 dB mode taps the second stage while the
/// resonance loop still spans all four (this matches the measured peak gain of the reference).
struct Ladder {
    var s1: Float = 0, s2: Float = 0, s3: Float = 0, s4: Float = 0

    mutating func reset() { s1 = 0; s2 = 0; s3 = 0; s4 = 0 }

    @inline(__always)
    mutating func process(_ x: Float, G: Float, k: Float, twoPole: Bool) -> Float {
        let oneMinusG = 1 - G
        let g2 = G * G
        let sigma = oneMinusG * (g2 * G * s1 + g2 * s2 + G * s3 + s4)
        var u = (x - k * sigma) / (1 + k * g2 * g2)
        u = fastTanh(u)
        var v = (u - s1) * G
        let y1 = v + s1
        s1 = y1 + v
        v = (y1 - s2) * G
        let y2 = v + s2
        s2 = y2 + v
        v = (y2 - s3) * G
        let y3 = v + s3
        s3 = y3 + v
        v = (y3 - s4) * G
        let y4 = v + s4
        s4 = y4 + v
        return twoPole ? y2 : y4
    }
}

/// 2x decimator: 31-tap half-band FIR (Kaiser, ~90 dB stopband) on interleaved pairs.
final class HalfBandDecimator {
    static let tapValues: [Float] = {
        let n = 31, beta = 8.0
        func i0(_ x: Double) -> Double {
            var s = 1.0, t = 1.0
            for k in 1..<30 { t *= (x / (2 * Double(k))) * (x / (2 * Double(k))); s += t }
            return s
        }
        var h = [Float](repeating: 0, count: n)
        let m = Double(n - 1) / 2
        for i in 0..<n {
            let x = Double(i) - m
            let sinc = x == 0 ? 0.5 : sin(Double.pi * x / 2) / (Double.pi * x)
            let w = i0(beta * sqrt(max(0, 1 - (x / m) * (x / m)))) / i0(beta)
            h[i] = Float(sinc * w)
        }
        let sum = h.reduce(0, +)
        return h.map { $0 / sum }
    }()

    private let taps: UnsafeMutablePointer<Float>
    private let history: UnsafeMutablePointer<Float>
    private var pos = 0

    init() {
        taps = .allocate(capacity: 31)
        for i in 0..<31 { taps[i] = HalfBandDecimator.tapValues[i] }
        history = .allocate(capacity: 64)
        history.initialize(repeating: 0, count: 64)
    }

    deinit {
        taps.deallocate()
        history.deallocate()
    }

    func reset() { history.update(repeating: 0, count: 64) }

    /// Consumes two oversampled inputs, returns one output sample.
    @inline(__always)
    func process(_ a: Float, _ b: Float) -> Float {
        history[pos] = a; history[pos + 32] = a
        pos = (pos + 1) & 31
        history[pos] = b; history[pos + 32] = b
        pos = (pos + 1) & 31
        // the 31 newest samples are contiguous at pos+1 ... pos+31 (mirrored buffer)
        let h = history + (pos + 1)
        var acc: Float = 0
        for i in 0..<31 { acc += taps[i] * h[i] }
        return acc
    }
}

// MARK: - Envelope

/// Analog-style ADSR measured from the reference: RC attack overshooting towards 1.4,
/// exponential decay and release. Rates are per sample.
struct Envelope {
    enum Stage: UInt8 { case idle, attack, decay, release }
    var stage: Stage = .idle
    var level: Float = 0

    @inline(__always)
    mutating func gateOn() { stage = .attack }

    @inline(__always)
    mutating func gateOff() { if stage != .idle { stage = .release } }

    @inline(__always)
    mutating func process(attackCoef: Float, decayCoef: Float, sustain: Float, releaseCoef: Float) -> Float {
        switch stage {
        case .idle:
            return 0
        case .attack:
            level += (Float(VoiceMath.attackTarget) - level) * attackCoef
            if level >= 1 { level = 1; stage = .decay }
        case .decay:
            level = sustain + (level - sustain) * decayCoef
        case .release:
            level *= releaseCoef
            if level < 1e-5 { level = 0; stage = .idle }
        }
        return level
    }

    /// Per-sample coefficients for display times in milliseconds.
    static func attackCoef(ms: Double, sampleRate: Double) -> Float {
        let tau = max(ms, 0.05) * VoiceMath.attackTauFactor * 0.001 * sampleRate
        return Float(1 - Foundation.exp(-1 / tau))
    }

    static func decayCoef(ms: Double, sampleRate: Double) -> Float {
        let tau = max(ms, 0.05) / VoiceMath.decayTauDivisor * 0.001 * sampleRate
        return Float(Foundation.exp(-1 / tau))
    }

    static func releaseCoef(ms: Double, sampleRate: Double) -> Float {
        let tau = max(ms, 0.05) / VoiceMath.releaseTauDivisor * 0.001 * sampleRate
        return Float(Foundation.exp(-1 / tau))
    }
}
