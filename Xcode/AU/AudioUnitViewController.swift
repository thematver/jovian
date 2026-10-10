import CoreAudioKit
import SwiftUI

/// Entry point of the extension: the host asks it for the Audio Unit and its view.
public final class AudioUnitViewController: AUViewController, AUAudioUnitFactory {
    private var audioUnit: Jovian8AudioUnit?
    private var hosting: NSHostingView<RootView>?

    public override func loadView() {
        view = NSView(frame: NSRect(x: 0, y: 0, width: 1180, height: 620))
        preferredContentSize = NSSize(width: 1180, height: 620)
        if audioUnit != nil { attachUI() }
    }

    public func createAudioUnit(with componentDescription: AudioComponentDescription) throws -> AUAudioUnit {
        let unit = try Jovian8AudioUnit(componentDescription: componentDescription, options: [])
        audioUnit = unit
        DispatchQueue.main.async { [weak self] in
            if self?.isViewLoaded == true { self?.attachUI() }
        }
        return unit
    }

    private func attachUI() {
        guard hosting == nil, let unit = audioUnit else { return }
        let host = NSHostingView(rootView: RootView(model: ParamModel(unit: unit)))
        host.frame = view.bounds
        host.autoresizingMask = [.width, .height]
        view.addSubview(host)
        hosting = host
    }
}
