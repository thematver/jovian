import XCTest
@testable import Jovian8Core

final class ParamTests: XCTestCase {
    func testTableLoads() {
        XCTAssertEqual(ParamTable.count, 419)
        XCTAssertEqual(Set(ParamTable.all.map(\.id)).count, ParamTable.count)
    }

    /// Curves must match the display values read from the original plugin.
    func testCurvesMatchReference() {
        func phys(_ id: String, _ x: Double) -> Double { ParamTable.spec(id).physical(x) }
        XCTAssertEqual(phys("LFO_RateUnSynced", 0.5), 1.53, accuracy: 0.01)
        XCTAssertEqual(phys("Env1_Attack", 0.5), 541, accuracy: 1)
        XCTAssertEqual(phys("Env1_Sustain", 0.25), 19.1, accuracy: 0.1)
        XCTAssertEqual(phys("OscEnvAmount", 0.25), 6.17, accuracy: 0.01)
        XCTAssertEqual(phys("HPF_Cutoff", 0.25), 366, accuracy: 1)
        XCTAssertEqual(phys("Volume", 0.5), -10.9, accuracy: 0.05)
        XCTAssertEqual(phys("ModMatrixAmount_Src1_Dest1", 0.75), 0.119, accuracy: 0.001)
        XCTAssertEqual(phys("Osc2Coarse", 0.325), 12)        // index 12 = 0 semitones
        XCTAssertEqual(phys("Osc1Coarse", 0.25), 1)          // 8'
        for s in ParamTable.all where !s.isDiscrete {
            for x in stride(from: 0.0, through: 1.0, by: 0.125) {
                XCTAssertEqual(s.normalized(fromPhysical: s.physical(x)), x, accuracy: 1e-6, s.id)
            }
        }
    }

    func testDiscreteStorageMatchesJup8V() {
        // Jup-8 V stores index / (count - 1) and reads min(floor(x * count), count - 1)
        for n in 2...54 {
            for i in 0..<n {
                XCTAssertEqual(ParamSpec.index(ParamSpec.normalized(index: i, count: n), count: n), i)
            }
        }
        XCTAssertEqual(ParamSpec.index(0.66666669, count: 4), 2)   // AssignMode Poly 1
    }

    func testFactoryPatchesAreValid() {
        XCTAssertGreaterThan(FactoryPatches.all.count, 5)
        for p in FactoryPatches.all {
            XCTAssertEqual(p.values.count, ParamTable.count, p.name)
            for (k, v) in p.values { XCTAssert((0...1).contains(v), "\(p.name) \(k) = \(v)") }
        }
    }
}
