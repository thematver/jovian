import AppKit
import AVFoundation
import Jovian8Core
import SwiftUI
import UniformTypeIdentifiers

/// Container app: installs the Audio Unit extension (macOS registers it when the app is present)
/// and offers a batch converter between Jup-8 V presets and .jup4x exports.
@main
struct Jovian8App: App {
    var body: some Scene {
        WindowGroup("Jovian-8") { ContentView().frame(minWidth: 640, minHeight: 460) }
            .windowResizability(.contentSize)
    }
}

struct ContentView: View {
    @State private var log: [String] = []
    @State private var registered = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("JOVIAN-8").font(.system(size: 28, weight: .heavy)).tracking(6)
            Text("Jupiter-8 style polyphonic synthesizer (Audio Unit v3) with Jup-8 V preset compatibility.")
                .foregroundStyle(.secondary)
            GroupBox("Use in Logic Pro") {
                VStack(alignment: .leading, spacing: 6) {
                    Text("1. Keep this app in /Applications and open it once — macOS then registers the plug-in.")
                    Text("2. In Logic: Settings › Plug-in Manager › Reset & Rescan Selection if it does not show up.")
                    Text("3. Software Instrument track › Instrument slot › AU Instruments › Jovian › Jovian-8.")
                    Label(registered ? "Audio Unit registered" : "Audio Unit not registered yet",
                          systemImage: registered ? "checkmark.circle.fill" : "exclamationmark.triangle")
                        .foregroundStyle(registered ? .green : .orange)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            GroupBox("Preset converter") {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Drop Jup-8 V preset files or .jup4x archives here, or pick them, to write a .jup4x bank that Jup-8 V can import.")
                    HStack {
                        Button("Choose files…") { choose() }
                        Button("Export all built-in patches") { exportBuiltIns() }
                        Button("Open ~/Music/Jovian-8") {
                            try? FileManager.default.createDirectory(at: PresetLocations.exportFolder, withIntermediateDirectories: true)
                            NSWorkspace.shared.open(PresetLocations.userFolder)
                        }
                    }
                    ScrollView {
                        VStack(alignment: .leading) { ForEach(log.indices, id: \.self) { Text(log[$0]).font(.system(size: 11, design: .monospaced)) } }
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .frame(height: 120)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(20)
        .onAppear(perform: checkRegistration)
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            for p in providers {
                _ = p.loadObject(ofClass: URL.self) { url, _ in if let url { DispatchQueue.main.async { convert([url]) } } }
            }
            return true
        }
    }

    private func checkRegistration() {
        let found = AVAudioUnitComponentManager.shared().components(matching: AUComponent.description)
        registered = !found.isEmpty
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.allowsMultipleSelection = true
        panel.canChooseDirectories = true
        panel.directoryURL = PresetLocations.arturiaRoot
        if panel.runModal() == .OK { convert(panel.urls) }
    }

    private func convert(_ urls: [URL]) {
        var states = [PatchState]()
        for url in urls {
            let files: [URL]
            if (try? url.resourceValues(forKeys: [.isDirectoryKey]))?.isDirectory == true {
                files = (FileManager.default.enumerator(at: url, includingPropertiesForKeys: nil)?.allObjects as? [URL]) ?? []
            } else { files = [url] }
            for f in files {
                guard let data = try? Data(contentsOf: f), let presets = try? Jup4xArchive.read(data) else { continue }
                states += presets.map(PatchState.init(preset:))
            }
        }
        guard !states.isEmpty else { log.append("No Jup-8 V presets found."); return }
        do {
            let name = states.count == 1 ? states[0].name : "Jovian-8 Bank \(states.count)"
            let url = try PresetLibrary.export(states, fileName: name)
            log.append("Wrote \(states.count) preset(s) to \(url.path)")
            NSWorkspace.shared.activateFileViewerSelecting([url])
        } catch { log.append("Failed: \(error.localizedDescription)") }
    }

    private func exportBuiltIns() {
        do {
            let url = try PresetLibrary.export(FactoryPatches.all, fileName: "Jovian-8 Built-in")
            log.append("Wrote \(FactoryPatches.all.count) built-in patches to \(url.path)")
            NSWorkspace.shared.activateFileViewerSelecting([url])
        } catch { log.append("Failed: \(error.localizedDescription)") }
    }
}

enum AUComponent {
    static let description = AudioComponentDescription(
        componentType: kAudioUnitType_MusicDevice, componentSubType: 0x4A56_3865,
        componentManufacturer: 0x4A6F_766E, componentFlags: 0, componentFlagsMask: 0)
}
