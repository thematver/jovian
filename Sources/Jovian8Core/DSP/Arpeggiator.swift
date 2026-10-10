import Foundation

/// Jup-8 V arpeggiator: Up, Down, Ordered, Reversed, Inc (up/down repeating the ends),
/// Exc (up/down without repeating), Random; 1-4 octaves; 50 % gate. Fixed capacity, no allocation.
struct Arpeggiator {
    static let capacity = 32
    var notes = [Int](repeating: 0, count: Arpeggiator.capacity)        // press order
    var velocities = [Float](repeating: 0, count: Arpeggiator.capacity)
    var count = 0
    private var sequence = [Int](repeating: 0, count: Arpeggiator.capacity * 4 * 2)
    private var seqVel = [Float](repeating: 0, count: Arpeggiator.capacity * 4 * 2)
    private var seqLength = 0
    private var position = -1
    private var phase = 1.0
    private var gateOpen = false
    private(set) var sounding: Int?
    private var rng = Noise(state: 0xA5A5_1234)
    private var dirty = true
    private var builtMode = -1, builtOctaves = -1

    mutating func reset() {
        count = 0; seqLength = 0; position = -1; phase = 1; gateOpen = false; sounding = nil; dirty = true
    }

    mutating func add(note: Int, velocity: Float) {
        remove(note: note)
        guard count < Arpeggiator.capacity else { return }
        notes[count] = note; velocities[count] = velocity; count += 1
        if count == 1 { phase = 1; position = -1 }   // first key starts the pattern immediately
        dirty = true
    }

    mutating func remove(note: Int) {
        var i = 0
        while i < count {
            if notes[i] == note {
                for j in i..<(count - 1) { notes[j] = notes[j + 1]; velocities[j] = velocities[j + 1] }
                count -= 1
                dirty = true
            } else { i += 1 }
        }
    }

    mutating func keepOnly(_ isDown: (Int) -> Bool) {
        var i = 0
        while i < count { if !isDown(notes[i]) { remove(note: notes[i]) } else { i += 1 } }
    }

    private mutating func rebuild(mode: Int, octaves: Int) {
        dirty = false; builtMode = mode; builtOctaves = octaves
        // base list: sorted ascending, or press order for Ordered/Reversed
        var base = (0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
        var baseV = base
        let n = count
        withUnsafeMutableBytes(of: &base) { rb in
            withUnsafeMutableBytes(of: &baseV) { rv in
                let b = rb.bindMemory(to: Int.self), bv = rv.bindMemory(to: Int.self)
                for i in 0..<n { b[i] = notes[i]; bv[i] = i }
                if mode != 2 && mode != 3 {   // sort ascending (insertion sort, n <= 32)
                    for i in 1..<max(n, 1) {
                        var j = i
                        while j > 0 && b[j - 1] > b[j] { b.swapAt(j - 1, j); bv.swapAt(j - 1, j); j -= 1 }
                    }
                }
                // expand over octaves (ascending)
                var len = 0
                for o in 0..<octaves {
                    for i in 0..<n { sequence[len] = b[i] + 12 * o; seqVel[len] = velocities[bv[i]]; len += 1 }
                }
                switch mode {
                case 1, 3:   // Down, Reversed
                    sequence.withUnsafeMutableBufferPointer { s in
                        seqVel.withUnsafeMutableBufferPointer { v in
                            var a = 0, z = len - 1
                            while a < z { s.swapAt(a, z); v.swapAt(a, z); a += 1; z -= 1 }
                        }
                    }
                case 4, 5:   // Inc: up then down including both ends; Exc: without repeating ends
                    let inc = mode == 4
                    var k = inc ? len - 1 : len - 2
                    let stop = inc ? 0 : 1
                    var out = len
                    while k >= stop && out < sequence.count {
                        sequence[out] = sequence[k]; seqVel[out] = seqVel[k]; out += 1; k -= 1
                    }
                    len = out
                default: break
                }
                seqLength = len
            }
        }
    }

    /// Advances the clock; returns the note to stop and/or the note to start.
    mutating func advance(dt: Double, hz: Double, mode: Int, octaves: Int) -> (off: Int?, on: Int?, velocity: Float) {
        if count == 0 {
            let off = sounding
            sounding = nil; gateOpen = false; position = -1; phase = 1
            return (off, nil, 0)
        }
        if dirty || mode != builtMode || octaves != builtOctaves { rebuild(mode: mode, octaves: octaves) }
        var off: Int? = nil
        var on: Int? = nil
        var vel: Float = 0
        phase += dt * max(hz, 0.01)
        if gateOpen && phase >= 0.5 {
            off = sounding; sounding = nil; gateOpen = false
        }
        if phase >= 1 {
            phase -= phase.rounded(.down)
            if let s = sounding { off = s }
            guard seqLength > 0 else { return (off, nil, 0) }
            if mode == 6 {
                position = min(Int(abs(rng.next()) * Float(seqLength)), seqLength - 1)
            } else {
                position = (position + 1) % seqLength
            }
            let note = min(max(sequence[position], 0), 127)
            on = note; vel = seqVel[position]
            sounding = note; gateOpen = true
        }
        return (off, on, vel)
    }
}
