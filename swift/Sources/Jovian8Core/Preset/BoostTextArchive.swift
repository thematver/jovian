import Foundation

/// Reader/writer for the subset of the Boost.Serialization text archive format used by
/// Arturia presets: blank-separated number tokens and length-prefixed byte strings
/// (`<length> <bytes>`, exactly one blank after the length; bytes may contain anything).
struct BoostTextReader {
    enum Failure: Error, CustomStringConvertible {
        case unexpectedEnd(Int), notANumber(String, Int)
        var description: String {
            switch self {
            case .unexpectedEnd(let at): return "unexpected end of archive at byte \(at)"
            case .notANumber(let t, let at): return "expected a number, found '\(t)' at byte \(at)"
            }
        }
    }

    let bytes: [UInt8]
    var pos: Int

    init(_ data: Data, at offset: Int = 0) {
        bytes = [UInt8](data)
        pos = offset
    }

    var isAtEnd: Bool {
        var p = pos
        while p < bytes.count, bytes[p] == 0x20 || bytes[p] == 0x0A || bytes[p] == 0x0D { p += 1 }
        return p >= bytes.count
    }

    mutating func token() throws -> String {
        while pos < bytes.count, bytes[pos] == 0x20 || bytes[pos] == 0x0A || bytes[pos] == 0x0D { pos += 1 }
        guard pos < bytes.count else { throw Failure.unexpectedEnd(pos) }
        let start = pos
        while pos < bytes.count, bytes[pos] != 0x20, bytes[pos] != 0x0A { pos += 1 }
        return String(decoding: bytes[start..<pos], as: UTF8.self)
    }

    mutating func int() throws -> Int {
        let at = pos
        let t = try token()
        guard let v = Int(t) else { throw Failure.notANumber(t, at) }
        return v
    }

    mutating func double() throws -> Double {
        let at = pos
        let t = try token()
        guard let v = Double(t) else { throw Failure.notANumber(t, at) }
        return v
    }

    mutating func skip(_ n: Int) throws { for _ in 0..<n { _ = try token() } }

    mutating func data() throws -> Data {
        let n = try int()
        guard n >= 0 else { throw Failure.notANumber(String(n), pos) }
        pos += 1                                   // the single separating blank
        guard pos + n <= bytes.count else { throw Failure.unexpectedEnd(pos) }
        defer { pos += n }
        return Data(bytes[pos..<(pos + n)])
    }

    mutating func string() throws -> String { String(decoding: try data(), as: UTF8.self) }
}

/// Like Boost's text_oarchive, every item after the first is preceded by one blank and the
/// archive ends with a line feed.
struct BoostTextWriter {
    private(set) var data = Data()
    private var first = true

    private mutating func item(_ s: String) {
        if !first { data.append(0x20) }
        first = false
        data.append(contentsOf: Array(s.utf8))
    }

    mutating func int(_ v: Int) { item(String(v)) }

    mutating func ints(_ v: [Int]) { for x in v { int(x) } }

    /// Floats are written like C++ streams with 8 significant digits ("0.33333334", "1", "0").
    mutating func float(_ v: Double) { item(BoostTextWriter.format(v)) }

    mutating func string(_ s: String) { bytes(Data(s.utf8)) }

    mutating func bytes(_ d: Data) {
        item(String(d.count))
        data.append(0x20)
        data.append(d)
    }

    mutating func finish() { data.append(0x0A) }

    static func format(_ v: Double) -> String {
        let f = Float(v)
        if f == 0 { return "0" }
        return String(format: "%.8g", Double(f))
    }
}
