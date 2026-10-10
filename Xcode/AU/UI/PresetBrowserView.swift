import AppKit
import Jovian8Core
import SwiftUI
import UniformTypeIdentifiers

struct PresetBrowserView: View {
    @ObservedObject var model: ParamModel
    @State private var entries: [LibraryEntry] = []
    @State private var source = "All"
    @State private var search = ""
    @State private var selection: String?
    @State private var scanning = false

    private static let builtIn = "Jovian-8 Built-in"

    private var rows: [(id: String, name: String, bank: String, source: String)] {
        var r = [(id: String, name: String, bank: String, source: String)]()
        if source == "All" || source == Self.builtIn {
            r += FactoryPatches.all.enumerated().map { ("builtin#\($0.offset)", $0.element.name, "Jovian-8", Self.builtIn) }
        }
        r += entries.filter { source == "All" || $0.source.rawValue == source }.map { ($0.id, $0.name, $0.bank, $0.source.rawValue) }
        if !search.isEmpty { r = r.filter { $0.name.localizedCaseInsensitiveContains(search) || $0.bank.localizedCaseInsensitiveContains(search) } }
        return r
    }

    var body: some View {
        HStack(alignment: .top, spacing: 0) {
            VStack(alignment: .leading, spacing: 6) {
                HStack {
                    Picker("", selection: $source) {
                        Text("All").tag("All")
                        Text(Self.builtIn).tag(Self.builtIn)
                        ForEach([LibraryEntry.Source.arturiaFactory, .arturiaUser, .jovian], id: \.self) { Text($0.rawValue).tag($0.rawValue) }
                    }
                    .labelsHidden().frame(width: 190)
                    TextField("Search", text: $search).textFieldStyle(.roundedBorder).frame(width: 200)
                    Button { rescan() } label: { Image(systemName: "arrow.clockwise") }.help("Rescan preset folders")
                    if scanning { ProgressView().controlSize(.small) }
                }
                List(rows, id: \.id, selection: $selection) { r in
                    HStack {
                        Text(r.name).font(.system(size: 11))
                        Spacer()
                        Text(r.bank).font(.system(size: 10)).foregroundStyle(.secondary)
                        Text(r.source).font(.system(size: 9)).foregroundStyle(.tertiary).frame(width: 110, alignment: .trailing)
                    }
                    .contentShape(Rectangle())
                }
                .onChange(of: selection) { _, id in if let id { open(id) } }
                .frame(minWidth: 560)
                Text("\(rows.count) presets. Jup-8 V presets are read from \(PresetLocations.arturiaRoot.path); drop .jup4x files into ~/Music/Jovian-8 to see them here.")
                    .font(.system(size: 9)).foregroundStyle(Palette.dim)
            }
            .padding(10)
            Divider()
            VStack(alignment: .leading, spacing: 10) {
                Text("CURRENT PATCH").font(.system(size: 9, weight: .semibold)).tracking(1.2).foregroundStyle(Palette.text)
                TextField("Name", text: $model.patchName, onCommit: model.commitName).textFieldStyle(.roundedBorder)
                TextField("Bank", text: $model.patchBank, onCommit: model.commitName).textFieldStyle(.roundedBorder)
                Button("Export to Jup-8 V (.jup4x)") { export() }.controlSize(.large)
                Button("Import .jup4x / preset file…") { importFile() }
                Button("Show export folder") { NSWorkspace.shared.open(PresetLocations.exportFolder) }
                Text("In Jup-8 V open the preset browser, choose Import and pick the exported .jup4x. Patches imported from Jup-8 V keep their sequencer, mod-mixer and MIDI-mapping data, so exporting them gives the original settings back.")
                    .font(.system(size: 10)).foregroundStyle(Palette.dim).fixedSize(horizontal: false, vertical: true)
                if !model.status.isEmpty {
                    Text(model.status).font(.system(size: 10)).foregroundStyle(Palette.yellow).fixedSize(horizontal: false, vertical: true)
                }
                Spacer()
            }
            .padding(12)
            .frame(width: 300)
        }
        .onAppear { if entries.isEmpty { rescan() } }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            for p in providers {
                _ = p.loadObject(ofClass: URL.self) { url, _ in
                    if let url { DispatchQueue.main.async { importURL(url) } }
                }
            }
            return true
        }
    }

    private func rescan() {
        scanning = true
        DispatchQueue.global(qos: .userInitiated).async {
            let found = PresetLibrary.scan()
            DispatchQueue.main.async {
                entries = found
                scanning = false
                if found.isEmpty { model.status = "No Jup-8 V presets found (is Jup-8 V installed?)." }
            }
        }
    }

    private func open(_ id: String) {
        if id.hasPrefix("builtin#"), let i = Int(id.dropFirst(8)), i < FactoryPatches.all.count {
            model.load(FactoryPatches.all[i])
            return
        }
        guard let e = entries.first(where: { $0.id == id }) else { return }
        do { model.load(try e.load()) } catch { model.status = "Could not open \(e.name): \(error)" }
    }

    private func export() {
        model.commitName()
        let state = model.unit.currentPatch()
        do {
            let url = try PresetLibrary.export([state], fileName: state.name)
            model.status = "Exported to \(url.path)"
            NSWorkspace.shared.activateFileViewerSelecting([url])
        } catch {
            model.status = "Export failed: \(error.localizedDescription)"
        }
    }

    private func importFile() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.begin { r in
            if r == .OK, let url = panel.url { importURL(url) }
        }
    }

    private func importURL(_ url: URL) {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        do {
            let presets = try Jup4xArchive.read(try Data(contentsOf: url))
            guard let first = presets.first else { model.status = "No Jup-8 V preset in \(url.lastPathComponent)"; return }
            model.load(PatchState(preset: first))
            if presets.count > 1 {
                // keep the whole bank available in the browser
                let dest = PresetLocations.userFolder.appendingPathComponent(url.lastPathComponent)
                try? FileManager.default.createDirectory(at: PresetLocations.userFolder, withIntermediateDirectories: true)
                try? FileManager.default.copyItem(at: url, to: dest)
                model.status = "Loaded \(first.name); \(presets.count) presets copied to ~/Music/Jovian-8"
                rescan()
            }
        } catch {
            model.status = "Import failed: \(error)"
        }
    }
}
