import XCTest
@testable import Jovian8Core

final class PresetTests: XCTestCase {
    func testWriteReadRoundTrip() throws {
        var state = FactoryPatches.all[1]
        state.extras["Seq_Swing"] = 0.25
        state.blobs["__Mapped__0"] = Data([1, 0, 0, 0])
        let preset = state.arturiaPreset()
        let bytes = preset.serialized()
        XCTAssert(String(decoding: bytes.prefix(40), as: UTF8.self).hasPrefix("22 serialization::archive 10 0 7 0 7"))
        XCTAssertEqual(bytes.last, 0x0A)
        let back = try ArturiaPreset.parse(bytes)
        XCTAssertEqual(back.name, state.name)
        XCTAssertEqual(back.instrumentID, 58)
        XCTAssertEqual(back.blobs["__Mapped__0"], Data([1, 0, 0, 0]))
        let state2 = PatchState(preset: back)
        for (k, v) in state.values { XCTAssertEqual(state2.values[k]!, v, accuracy: 1e-7, k) }
        XCTAssertEqual(state2.extras["Seq_Swing"]!, 0.25, accuracy: 1e-7)
        // a full parameter set is written, like Jup-8 V itself does
        XCTAssertGreaterThanOrEqual(back.params.count, 1535)
    }

    func testEmptyStringsAndSpacesSurvive() throws {
        var p = ArturiaPreset(name: "My  Patch ", pack: "", author: "", comment: "two\nlines")
        p.params = ["LpfCut": 0.5]
        let back = try ArturiaPreset.parse(p.serialized())
        XCTAssertEqual(back.name, "My  Patch ")
        XCTAssertEqual(back.pack, "")
        XCTAssertEqual(back.comment, "two\nlines")
        XCTAssertEqual(back.params["LpfCut"], 0.5)
    }

    func testJup4xContainer() throws {
        let states = Array(FactoryPatches.all.prefix(3))
        let zip = Jup4xArchive.make(states.map { $0.arturiaPreset() })
        // minimal ZIP: stored entries, version 20, no flags, no extra fields
        XCTAssertEqual(Array(zip.prefix(4)), [0x50, 0x4B, 0x03, 0x04])
        XCTAssertEqual(zip[4], 20); XCTAssertEqual(zip[6], 0); XCTAssertEqual(zip[8], 0)
        let entries = try MinimalZip.entries(of: zip)
        XCTAssertEqual(entries.map(\.path), states.map { "Jup-8/User/Jovian-8/\($0.name)" })
        let back = try Jup4xArchive.read(zip)
        XCTAssertEqual(back.map(\.name), states.map(\.name))
    }

    func testFactoryLibraryIfInstalled() throws {
        let url = PresetLocations.arturiaRoot.appendingPathComponent("Factory/Factory/Default")
        guard let data = try? Data(contentsOf: url) else { throw XCTSkip("Jup-8 V not installed") }
        let p = try ArturiaPreset.parse(data)
        XCTAssertEqual(p.name, "Default")
        // the writer reproduces Jup-8 V's own file byte for byte
        XCTAssertEqual(p.serialized(), data)
    }
}
