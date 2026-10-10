import Foundation

/// The plugin's saved state. Modelled parameters live in `values` (normalized, keyed by the Jup-8 V
/// id); everything else that came in with an imported Jup-8 V preset (sequencer, mod mixer, macros,
/// MIDI maps, ...) is carried along untouched so that exporting gives the original back.
public struct PatchState: Codable, Sendable, Equatable {
    public var name: String
    public var pack: String
    public var author: String
    public var comment: String
    public var version: String
    public var meta: [String: String]
    public var values: [String: Double]
    public var extras: [String: Double]
    public var blobs: [String: Data]

    public init(name: String = "Init", pack: String = "User", author: String = "", comment: String = "") {
        self.name = name
        self.pack = pack
        self.author = author
        self.comment = comment
        self.version = ArturiaPreset.defaultVersion
        self.meta = [:]
        self.values = Dictionary(uniqueKeysWithValues: ParamTable.all.map { ($0.id, $0.defaultValue) })
        self.extras = [:]
        self.blobs = [:]
    }

    /// Imports a Jup-8 V preset.
    public init(preset p: ArturiaPreset) {
        self.init(name: p.name, pack: p.pack, author: p.author, comment: p.comment)
        meta = p.meta
        blobs = p.blobs
        for s in ParamTable.all {
            values[s.id] = p.normalizedForCurrentVersion(s.id) ?? ArturiaTemplate.values[s.id] ?? s.defaultValue
        }
        extras = p.params.filter { ParamTable.addressByID[$0.key] == nil }
        // keep the original version only for what we pass through; modelled values are converted
        version = ArturiaPreset.defaultVersion
    }

    /// Builds a complete Jup-8 V preset: neutral template < carried-over extras < modelled values.
    public func arturiaPreset() -> ArturiaPreset {
        var params = ArturiaTemplate.values
        for (k, v) in extras { params[k] = v }
        for (k, v) in values where ParamTable.addressByID[k] != nil { params[k] = min(max(v, 0), 1) }
        var m = meta
        if m["OriginalPresetName"] == nil { m["OriginalPresetName"] = name }
        if m["OriginalPackName"] == nil { m["OriginalPackName"] = pack }
        if m["Type"] == nil { m["Type"] = "Custom" }
        // exports land in Jup-8 V's user area; don't pose as one of its factory banks
        let bank = ["Factory", "Vintage Factory", ""].contains(pack) ? "Jovian-8" : pack
        var p = ArturiaPreset(name: name, pack: bank, author: author, comment: comment, params: params, meta: m, blobs: blobs)
        p.version = version
        return p
    }

    /// Copies the modelled values into the live parameter store.
    public func apply(to store: ParamStore) {
        for s in ParamTable.all {
            store[s.address] = Float(values[s.id] ?? s.defaultValue)
        }
    }

    /// Reads the modelled values back from the live parameter store.
    public mutating func capture(from store: ParamStore) {
        for s in ParamTable.all { values[s.id] = Double(store[s.address]) }
    }

    public func encoded() throws -> Data { try PropertyListEncoder().encode(self) }

    public static func decode(_ data: Data) throws -> PatchState { try PropertyListDecoder().decode(PatchState.self, from: data) }
}

extension ArturiaTemplate {
    static let values: [String: Double] = {
        var d = [String: Double]()
        for line in text.split(separator: "\n") {
            let parts = line.split(separator: "\t")
            if parts.count == 2, let v = Double(parts[1]) { d[String(parts[0])] = v }
        }
        return d
    }()
}

/// Locations of preset files.
public enum PresetLocations {
    /// Where Jup-8 V keeps its factory and user presets.
    public static let arturiaRoot = URL(fileURLWithPath: "/Library/Arturia/Presets/Jup-8 V4", isDirectory: true)

    /// The user's real home folder, also from inside an app sandbox.
    public static var realHome: URL {
        if let pw = getpwuid(getuid()), let dir = pw.pointee.pw_dir {
            return URL(fileURLWithPath: String(cString: dir), isDirectory: true)
        }
        return FileManager.default.homeDirectoryForCurrentUser
    }

    /// ~/Music/Jovian-8: exports are written to Exports/, and any .jup4x or preset file dropped
    /// anywhere in this folder shows up in the browser.
    public static var userFolder: URL { realHome.appendingPathComponent("Music/Jovian-8", isDirectory: true) }
    public static var exportFolder: URL { userFolder.appendingPathComponent("Exports", isDirectory: true) }
}

/// A preset found on disk.
public struct LibraryEntry: Hashable, Sendable, Identifiable {
    public var id: String { url.path + "#" + String(index) }
    public let name: String
    public let bank: String
    public let source: Source
    public let url: URL
    /// Index inside a multi-preset archive.
    public let index: Int

    public enum Source: String, Sendable { case arturiaFactory = "Jup-8 V Factory", arturiaUser = "Jup-8 V User", jovian = "Jovian-8" }

    public func load() throws -> PatchState {
        let data = try Data(contentsOf: url)
        let presets = try Jup4xArchive.read(data)
        guard index < presets.count else { throw ArturiaPreset.ParseError.noParameterMap }
        return PatchState(preset: presets[index])
    }
}

public enum PresetLibrary {
    /// Lists installed Jup-8 V presets and the user's own folder. Unreadable locations are skipped.
    public static func scan() -> [LibraryEntry] {
        var out = [LibraryEntry]()
        let fm = FileManager.default
        if let e = fm.enumerator(at: PresetLocations.arturiaRoot, includingPropertiesForKeys: [.isRegularFileKey]) {
            for case let url as URL in e where url.pathExtension.isEmpty {
                guard (try? url.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true else { continue }
                let rel = url.path.replacingOccurrences(of: PresetLocations.arturiaRoot.path + "/", with: "")
                let parts = rel.split(separator: "/")
                guard parts.count >= 2, parts.first != "Tutorials" else { continue }
                let source: LibraryEntry.Source = parts.first == "Factory" ? .arturiaFactory : .arturiaUser
                out.append(LibraryEntry(name: url.lastPathComponent, bank: String(parts[parts.count - 2]), source: source, url: url, index: 0))
            }
        }
        if let e = fm.enumerator(at: PresetLocations.userFolder, includingPropertiesForKeys: [.isRegularFileKey]) {
            for case let url as URL in e {
                guard (try? url.resourceValues(forKeys: [.isRegularFileKey]))?.isRegularFile == true else { continue }
                if url.pathExtension.lowercased() == Jup4xArchive.fileExtension {
                    if let data = try? Data(contentsOf: url), let presets = try? Jup4xArchive.read(data) {
                        for (i, p) in presets.enumerated() {
                            out.append(LibraryEntry(name: p.name, bank: p.pack, source: .jovian, url: url, index: i))
                        }
                    }
                } else if url.pathExtension.isEmpty,
                          let h = try? FileHandle(forReadingFrom: url), let head = try? h.read(upToCount: 40),
                          ArturiaPreset.looksLikePreset(head) {
                    out.append(LibraryEntry(name: url.lastPathComponent, bank: url.deletingLastPathComponent().lastPathComponent,
                                            source: .jovian, url: url, index: 0))
                }
            }
        }
        return out.sorted { ($0.source.rawValue, $0.bank, $0.name) < ($1.source.rawValue, $1.bank, $1.name) }
    }

    /// Writes `.jup4x` exports for Jup-8 V's Import command; returns the file URL.
    @discardableResult
    public static func export(_ states: [PatchState], fileName: String, to folder: URL = PresetLocations.exportFolder) throws -> URL {
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let url = folder.appendingPathComponent(Jup4xArchive.sanitize(fileName)).appendingPathExtension(Jup4xArchive.fileExtension)
        try Jup4xArchive.make(states.map { $0.arturiaPreset() }).write(to: url, options: .atomic)
        return url
    }
}
