import Compression
import Foundation

/// The `.jup4x` file Jup-8 V writes with "Export Preset/Bank" and reads with "Import": a ZIP whose
/// entries are preset files at `<product>/User/<bank>/<preset>`. Jup-8 V's importer only accepts a
/// minimal container: stored entries, no flags, no extra fields, no directory entries.
public enum Jup4xArchive {
    public static let fileExtension = "jup4x"
    /// Product folder inside the archive (the instrument's display name in Arturia's database).
    public static let productFolder = "Jup-8"

    public enum Failure: Error, CustomStringConvertible {
        case notAZip, unsupportedCompression(Int), corrupt
        public var description: String {
            switch self {
            case .notAZip: return "not a ZIP archive"
            case .unsupportedCompression(let m): return "unsupported ZIP compression method \(m)"
            case .corrupt: return "the archive is damaged"
            }
        }
    }

    /// Builds an export archive; the bank folder is each preset's `pack`.
    public static func make(_ presets: [ArturiaPreset]) -> Data {
        var zip = MinimalZip()
        var used = Set<String>()
        for p in presets {
            let bank = sanitize(p.pack.isEmpty ? "User" : p.pack)
            var name = sanitize(p.name.isEmpty ? "Untitled" : p.name)
            var path = "\(productFolder)/User/\(bank)/\(name)"
            var n = 2
            while used.contains(path) { name = sanitize(p.name) + " \(n)"; path = "\(productFolder)/User/\(bank)/\(name)"; n += 1 }
            used.insert(path)
            zip.add(path: path, data: p.serialized())
        }
        return zip.finish()
    }

    /// Reads every preset in an archive (any folder layout). Also accepts a bare preset file.
    public static func read(_ data: Data) throws -> [ArturiaPreset] {
        if ArturiaPreset.looksLikePreset(data) { return [try ArturiaPreset.parse(data)] }
        var out = [ArturiaPreset]()
        for entry in try MinimalZip.entries(of: data) where ArturiaPreset.looksLikePreset(entry.data) {
            if var p = try? ArturiaPreset.parse(entry.data) {
                let parts = entry.path.split(separator: "/")
                if p.pack.isEmpty, parts.count >= 2 { p.pack = String(parts[parts.count - 2]) }
                out.append(p)
            }
        }
        return out
    }

    static func sanitize(_ s: String) -> String {
        var t = s.replacingOccurrences(of: "/", with: "-").replacingOccurrences(of: "\\", with: "-")
        t = t.replacingOccurrences(of: ":", with: "-").trimmingCharacters(in: .whitespacesAndNewlines)
        while t.hasPrefix(".") { t.removeFirst() }
        return t.isEmpty ? "Untitled" : t
    }
}

/// Minimal ZIP writer/reader (stored entries; the reader also inflates deflated ones).
struct MinimalZip {
    private var body = Data()
    private var central = Data()
    private var count = 0

    mutating func add(path: String, data: Data) {
        let name = Data(path.utf8)
        let crc = CRC32.checksum(data)
        let offset = UInt32(body.count)
        body.le32(0x0403_4B50); body.le16(20); body.le16(0); body.le16(0)
        body.le16(0); body.le16(0x21)                      // time 00:00, date 1980-01-01
        body.le32(crc); body.le32(UInt32(data.count)); body.le32(UInt32(data.count))
        body.le16(UInt16(name.count)); body.le16(0)
        body.append(name); body.append(data)

        central.le32(0x0201_4B50); central.le16(20); central.le16(20); central.le16(0); central.le16(0)
        central.le16(0); central.le16(0x21)
        central.le32(crc); central.le32(UInt32(data.count)); central.le32(UInt32(data.count))
        central.le16(UInt16(name.count)); central.le16(0); central.le16(0); central.le16(0); central.le16(0)
        central.le32(0); central.le32(offset)
        central.append(name)
        count += 1
    }

    func finish() -> Data {
        var out = body
        out.append(central)
        out.le32(0x0605_4B50); out.le16(0); out.le16(0)
        out.le16(UInt16(count)); out.le16(UInt16(count))
        out.le32(UInt32(central.count)); out.le32(UInt32(body.count)); out.le16(0)
        return out
    }

    struct Entry { let path: String; let data: Data }

    static func entries(of zip: Data) throws -> [Entry] {
        let b = [UInt8](zip)
        func u16(_ o: Int) -> Int { Int(b[o]) | Int(b[o + 1]) << 8 }
        func u32(_ o: Int) -> Int { u16(o) | u16(o + 2) << 16 }
        // end of central directory
        var eocd = -1
        var i = b.count - 22
        while i >= max(0, b.count - 22 - 65_535) {
            if u32(i) == 0x0605_4B50 { eocd = i; break }
            i -= 1
        }
        guard eocd >= 0 else { throw Jup4xArchive.Failure.notAZip }
        let total = u16(eocd + 10)
        var p = u32(eocd + 16)
        var out = [Entry]()
        for _ in 0..<total {
            guard p + 46 <= b.count, u32(p) == 0x0201_4B50 else { throw Jup4xArchive.Failure.corrupt }
            let method = u16(p + 10), csize = u32(p + 20), usize = u32(p + 24)
            let nlen = u16(p + 28), xlen = u16(p + 30), clen = u16(p + 32), local = u32(p + 42)
            let name = String(decoding: b[(p + 46)..<(p + 46 + nlen)], as: UTF8.self)
            p += 46 + nlen + xlen + clen
            guard local + 30 <= b.count, u32(local) == 0x0403_4B50 else { throw Jup4xArchive.Failure.corrupt }
            let start = local + 30 + u16(local + 26) + u16(local + 28)
            guard start + csize <= b.count else { throw Jup4xArchive.Failure.corrupt }
            if name.hasSuffix("/") { continue }
            let raw = Data(b[start..<(start + csize)])
            switch method {
            case 0: out.append(Entry(path: name, data: raw))
            case 8: out.append(Entry(path: name, data: try inflate(raw, size: usize)))
            default: throw Jup4xArchive.Failure.unsupportedCompression(method)
            }
        }
        return out
    }

    static func inflate(_ data: Data, size: Int) throws -> Data {
        guard size > 0 else { return Data() }
        var out = Data(count: size)
        let n = out.withUnsafeMutableBytes { dst in
            data.withUnsafeBytes { src in
                compression_decode_buffer(dst.bindMemory(to: UInt8.self).baseAddress!, size,
                                          src.bindMemory(to: UInt8.self).baseAddress!, data.count, nil, COMPRESSION_ZLIB)
            }
        }
        guard n == size else { throw Jup4xArchive.Failure.corrupt }
        return out
    }
}

enum CRC32 {
    static let table: [UInt32] = (0..<256).map { i -> UInt32 in
        var c = UInt32(i)
        for _ in 0..<8 { c = c & 1 != 0 ? 0xEDB8_8320 ^ (c >> 1) : c >> 1 }
        return c
    }

    static func checksum(_ data: Data) -> UInt32 {
        var c: UInt32 = 0xFFFF_FFFF
        for byte in data { c = table[Int((c ^ UInt32(byte)) & 0xFF)] ^ (c >> 8) }
        return c ^ 0xFFFF_FFFF
    }
}

private extension Data {
    mutating func le16(_ v: UInt16) { append(UInt8(v & 0xFF)); append(UInt8(v >> 8)) }
    mutating func le16(_ v: Int) { le16(UInt16(truncatingIfNeeded: v)) }
    mutating func le32(_ v: UInt32) { for s in stride(from: 0, to: 32, by: 8) { append(UInt8((v >> UInt32(s)) & 0xFF)) } }
}
