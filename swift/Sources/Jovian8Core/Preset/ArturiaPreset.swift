import Foundation

/// A Jup-8 V preset file: a Boost text archive with a header, a string map of browser metadata,
/// a map of normalized parameter values and a map of binary blobs (MIDI mappings).
///
/// Layout (single line, blank separated; strings are `<length> <bytes>`):
/// ```
/// 22 serialization::archive 10 0 7 0 7 <name> <pack> <instrument id> <author> <type>
/// 0 0 <tag count> 0 <tags...> 1 0 <comment> <unix time> <app version>
/// 0 0 0 0 0 0 0 <empty> 0 0 <meta count> 0 0 0 (<key> <value>)...
/// 0 0 0 7 0 0 0 0 0 0 <param count> 0 0 0 (<name> <value>)... <blob count> 0 (<key> <blob>)... \n
/// ```
public struct ArturiaPreset: Sendable, Equatable {
    public static let jup8InstrumentID = 58
    /// Written into new presets; selects the newest value mappings in Jup-8 V (>= 3.9.4).
    public static let defaultVersion = "4.6.2.5305"

    public var name: String
    public var pack: String
    public var instrumentID: Int
    public var author: String
    public var type: String
    public var tags: [String]
    public var comment: String
    public var timestamp: Int
    public var version: String
    public var meta: [String: String]
    public var params: [String: Double]
    public var blobs: [String: Data]

    public init(name: String, pack: String = "User", author: String = "", comment: String = "",
                params: [String: Double] = [:], meta: [String: String] = [:], blobs: [String: Data] = [:]) {
        self.name = name
        self.pack = pack
        self.instrumentID = ArturiaPreset.jup8InstrumentID
        self.author = author
        self.type = ""
        self.tags = []
        self.comment = comment
        self.timestamp = Int(Date().timeIntervalSince1970)
        self.version = ArturiaPreset.defaultVersion
        self.meta = meta
        self.params = params
        self.blobs = blobs
    }

    public enum ParseError: Error, CustomStringConvertible {
        case notAnArchive, noParameterMap, wrongInstrument(Int)
        public var description: String {
            switch self {
            case .notAnArchive: return "not a Jup-8 V preset (missing Boost archive header)"
            case .noParameterMap: return "no parameter block found in the preset"
            case .wrongInstrument(let id): return "preset belongs to another Arturia instrument (id \(id))"
            }
        }
    }

    public static func looksLikePreset(_ data: Data) -> Bool {
        data.starts(with: Array("22 serialization::archive 10 0 7".utf8))
    }

    /// Parses a preset file. Presets of other Arturia instruments are rejected unless `anyInstrument`.
    public static func parse(_ data: Data, anyInstrument: Bool = false) throws -> ArturiaPreset {
        guard looksLikePreset(data) else { throw ParseError.notAnArchive }
        var r = BoostTextReader(data)
        _ = try r.string()                         // "serialization::archive"
        try r.skip(5)                              // 10 0 7 0 7
        var p = ArturiaPreset(name: try r.string())
        p.pack = try r.string()
        p.instrumentID = try r.int()
        if !anyInstrument && p.instrumentID != jup8InstrumentID { throw ParseError.wrongInstrument(p.instrumentID) }
        p.author = try r.string()
        p.type = try r.string()
        try r.skip(2)
        let tagCount = try r.int()
        try r.skip(1)
        p.tags = try (0..<max(tagCount, 0)).map { _ in try r.string() }
        try r.skip(2)
        p.comment = try r.string()
        p.timestamp = try r.int()
        p.version = try r.string()
        try r.skip(7)
        _ = try r.data()
        try r.skip(2)
        let metaCount = try r.int()
        try r.skip(metaCount > 0 ? 3 : 1)
        var meta = [String: String]()
        for _ in 0..<max(metaCount, 0) {
            let k = try r.string()
            meta[k] = try r.string()
        }
        p.meta = meta
        // The parameter map normally follows "0 0 0 7 0 0 0 0 0 0"; layered presets embed extra
        // sub-headers there, so search for the map that parses cleanly up to the end of the file.
        guard let tail = findParameterMap(data, from: r.pos, metaWasEmpty: metaCount == 0) else {
            throw ParseError.noParameterMap
        }
        p.params = tail.params
        p.blobs = tail.blobs
        return p
    }

    private static func findParameterMap(_ data: Data, from start: Int, metaWasEmpty: Bool)
        -> (params: [String: Double], blobs: [String: Data])? {
        let b = [UInt8](data)
        var i = start
        while i < b.count - 8 {
            // candidate: "<digits> 0 0 0 <digit>" preceded by a blank
            if b[i - 1] == 0x20, b[i] >= 0x30, b[i] <= 0x39 {
                var j = i
                while j < b.count, b[j] >= 0x30, b[j] <= 0x39 { j += 1 }
                if j + 7 < b.count, b[j] == 0x20, b[j + 1] == 0x30, b[j + 2] == 0x20, b[j + 3] == 0x30,
                   b[j + 4] == 0x20, b[j + 5] == 0x30, b[j + 6] == 0x20,
                   let parsed = tryParseTail(data, at: i, metaWasEmpty: metaWasEmpty) {
                    return parsed
                }
                i = j
            }
            i += 1
        }
        return nil
    }

    private static func tryParseTail(_ data: Data, at offset: Int, metaWasEmpty: Bool)
        -> (params: [String: Double], blobs: [String: Data])? {
        for blobHeader in metaWasEmpty ? [3, 1] : [1, 3] {
            var r = BoostTextReader(data, at: offset)
            do {
                let n = try r.int()
                guard n > 0, n < 100_000 else { return nil }
                try r.skip(3)
                var params = [String: Double]()
                params.reserveCapacity(n)
                for _ in 0..<n {
                    let k = try r.string()
                    params[k] = try r.double()
                }
                let m = try r.int()
                guard m >= 0, m < 100_000 else { return nil }
                try r.skip(blobHeader)
                var blobs = [String: Data]()
                for _ in 0..<m {
                    let k = try r.string()
                    blobs[k] = try r.data()
                }
                if r.isAtEnd { return (params, blobs) }
            } catch {
                continue
            }
        }
        return nil
    }

    /// Serializes in the layout Jup-8 V writes (maps sorted by key, like std::map).
    public func serialized() -> Data {
        var w = BoostTextWriter()
        w.string("serialization::archive")
        w.ints([10, 0, 7, 0, 7])
        w.string(name)
        w.string(pack)
        w.int(instrumentID)
        w.string(author)
        w.string(type)
        w.ints([0, 0, tags.count, 0])
        for t in tags { w.string(t) }
        w.ints([1, 0])
        w.string(comment)
        w.int(timestamp)
        w.string(version)
        w.ints([0, 0, 0, 0, 0, 0, 0])
        w.string("")
        w.ints([0, 0])
        var metaOut = meta
        if metaOut.isEmpty { metaOut["Type"] = "Custom" }   // keeps the pair class header where Jup-8 V expects it
        w.ints([metaOut.count, 0, 0, 0])
        for k in metaOut.keys.sorted(by: Self.byteOrder) {
            w.string(k)
            w.string(metaOut[k]!)
        }
        w.ints([0, 0, 0, 7, 0, 0, 0, 0, 0, 0])
        w.ints([params.count, 0, 0, 0])
        for k in params.keys.sorted(by: Self.byteOrder) {
            w.string(k)
            w.float(params[k]!)
        }
        w.ints([blobs.count, 0])
        for k in blobs.keys.sorted(by: Self.byteOrder) {
            w.string(k)
            w.bytes(blobs[k]!)
        }
        w.finish()
        return w.data
    }

    static func byteOrder(_ a: String, _ b: String) -> Bool { a.utf8.lexicographicallyPrecedes(b.utf8) }

    /// Version-aware decode: presets saved before 3.9.4 used shorter ranges for a few parameters.
    /// Returns the value re-expressed in the current mapping.
    public func normalizedForCurrentVersion(_ id: String) -> Double? {
        guard let v = params[id] else { return nil }
        guard ArturiaPreset.versionLess(version, "3.9.4") else { return v }
        switch id {
        case "LFO_FadeDelay":
            // 0...3.8 s in old presets, 0...7.5 s now (same Exp(2.3) curve)
            let spec = ParamTable.spec(id)
            let seconds = 3.8 * Curve.exp(2.3).shape(v)
            return spec.normalized(fromPhysical: seconds)
        default:
            return v
        }
    }

    static func versionLess(_ a: String, _ b: String) -> Bool {
        let pa = a.split(separator: ".").map { Int($0) ?? 0 }, pb = b.split(separator: ".").map { Int($0) ?? 0 }
        for i in 0..<max(pa.count, pb.count) {
            let x = i < pa.count ? pa[i] : 0, y = i < pb.count ? pb[i] : 0
            if x != y { return x < y }
        }
        return false
    }
}
