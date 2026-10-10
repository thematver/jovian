import AudioToolbox
import Combine
import Foundation
import Jovian8Core

/// One observable parameter for SwiftUI. `value` is normalized (Jup-8 V storage) for every kind.
final class ParamCell: ObservableObject, Identifiable {
    let spec: ParamSpec
    var id: Int { spec.address }
    @Published private(set) var value: Double
    fileprivate weak var model: ParamModel?

    init(spec: ParamSpec, value: Double) {
        self.spec = spec
        self.value = value
    }

    var index: Int { ParamSpec.index(value, count: spec.itemCount) }
    var isOn: Bool { value >= 0.5 }
    var display: String { spec.displayString(value) }

    func set(_ v: Double) {
        let v = min(max(v, 0), 1)
        guard v != value else { return }
        value = v
        model?.send(self)
    }

    func set(index i: Int) { set(ParamSpec.normalized(index: i, count: spec.itemCount)) }

    func toggle() { set(isOn ? 0 : 1) }

    fileprivate func receive(_ v: Double) { if v != value { value = v } }
}

/// Bridges the AU parameter tree and the patch state to the views.
final class ParamModel: ObservableObject {
    let unit: Jovian8AudioUnit
    private(set) var cells: [ParamCell] = []
    private var byID: [String: ParamCell] = [:]
    private var token: AUParameterObserverToken?
    @Published var patchName: String
    @Published var patchBank: String
    @Published var status: String = ""

    init(unit: Jovian8AudioUnit) {
        self.unit = unit
        let store = unit.engine.params
        patchName = unit.patch.name
        patchBank = unit.patch.pack
        cells = ParamTable.all.map { ParamCell(spec: $0, value: Double(store[$0.address])) }
        for c in cells { c.model = self; byID[c.spec.id] = c }
        token = unit.parameterTree?.token(byAddingParameterObserver: { [weak self] address, _ in
            DispatchQueue.main.async {
                guard let self else { return }
                let a = Int(address)
                guard a < self.cells.count else { return }
                self.cells[a].receive(Double(self.unit.engine.params[a]))
            }
        })
        unit.onPatchLoaded = { [weak self] state in
            DispatchQueue.main.async {
                guard let self else { return }
                self.patchName = state.name
                self.patchBank = state.pack
                self.refreshAll()
            }
        }
    }

    deinit {
        if let token { unit.parameterTree?.removeParameterObserver(token) }
    }

    subscript(id: String) -> ParamCell {
        guard let c = byID[id] else { preconditionFailure("no parameter \(id)") }
        return c
    }

    fileprivate func send(_ cell: ParamCell) {
        guard let p = unit.parameterTree?.parameter(withAddress: AUParameterAddress(cell.spec.address)) else { return }
        let auValue: AUValue = cell.spec.isDiscrete ? AUValue(cell.index) : AUValue(cell.value)
        p.setValue(auValue, originator: token)
    }

    func refreshAll() {
        let store = unit.engine.params
        for c in cells { c.receive(Double(store[c.spec.address])) }
    }

    func load(_ state: PatchState) {
        unit.load(state)
        patchName = state.name
        patchBank = state.pack
        refreshAll()
        status = "Loaded \(state.name)"
    }

    func commitName() { unit.rename(name: patchName, pack: patchBank) }
}
