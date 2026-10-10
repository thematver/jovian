// Minimal Audio Unit host for testing the Jovian-8 editor the way Logic loads it (AUv2,
// in process, Cocoa view). Sends synthetic mouse events to the view and saves the pixels the
// plugin pushed to its CALayer.
//
//   auhost <out-dir> [script]
//   script lines: "wait <s>", "shot <name>", "click <x> <y>", "drag <x0> <y0> <x1> <y1>",
//                 "move <x> <y>", "param <id>", "dblclick <x> <y>", "wheel <x> <y> <dy>"
// Coordinates are view points from the top-left corner.
import AppKit
import AudioToolbox
import AVFoundation
import CoreAudioKit
import ImageIO
import UniformTypeIdentifiers

func fourcc(_ s: String) -> FourCharCode { s.utf8.reduce(0) { $0 << 8 | FourCharCode($1) } }

let args = CommandLine.arguments
let outDir = args.count > 1 ? args[1] : "/tmp/auhost"
let script = args.count > 2 ? (try? String(contentsOfFile: args[2], encoding: .utf8)) ?? "" : "wait 1.5\nshot initial"
try? FileManager.default.createDirectory(atPath: outDir, withIntermediateDirectories: true)

let app = NSApplication.shared
app.setActivationPolicy(.regular)
var window: NSWindow?
var unit: AVAudioUnit?
var hostView: NSView?

func save(_ image: CGImage, _ name: String) {
    let url = URL(fileURLWithPath: "\(outDir)/\(name).png") as CFURL
    guard let dest = CGImageDestinationCreateWithURL(url, UTType.png.identifier as CFString, 1, nil) else { return }
    CGImageDestinationAddImage(dest, image, nil)
    CGImageDestinationFinalize(dest)
    print("saved \(name).png \(image.width)x\(image.height)")
}

func findImage(_ layer: CALayer) -> CGImage? {
    if let c = layer.contents, CFGetTypeID(c as CFTypeRef) == CGImage.typeID { return (c as! CGImage) }
    for s in layer.sublayers ?? [] { if let i = findImage(s) { return i } }
    return nil
}

func point(_ x: Double, _ y: Double) -> NSPoint {
    guard let v = hostView, let w = window else { return .zero }
    let p = v.convert(NSPoint(x: x, y: v.isFlipped ? y : v.bounds.height - y), to: nil)
    _ = w
    return p
}

func mouse(_ type: NSEvent.EventType, _ x: Double, _ y: Double, clicks: Int = 1) {
    guard let w = window else { return }
    let e = NSEvent.mouseEvent(with: type, location: point(x, y), modifierFlags: [], timestamp: ProcessInfo.processInfo.systemUptime,
                               windowNumber: w.windowNumber, context: nil, eventNumber: 0, clickCount: clicks, pressure: 1)
    if let e { NSApp.postEvent(e, atStart: false) }
}

func param(_ id: String) {
    guard let tree = unit?.auAudioUnit.parameterTree else { print("no parameter tree"); return }
    let key = id.lowercased().replacingOccurrences(of: "_", with: " ")
    for p in tree.allParameters where p.displayName.lowercased() == key {
        print("param \(p.displayName) [\(p.identifier)] = \(p.value) '\(p.string(fromValue: nil))'")
    }
}

func run(_ lines: [Substring], _ i: Int) {
    guard i < lines.count else { DispatchQueue.main.asyncAfter(deadline: .now() + 0.2) { exit(0) }; return }
    let f = lines[i].split(separator: " ").map(String.init)
    var delay = 0.05
    switch f.first ?? "" {
    case "wait": delay = Double(f[1]) ?? 1
    case "shot":
        if let v = hostView, let img = findImage(v.layer ?? CALayer()) ?? v.subviews.compactMap({ findImage($0.layer ?? CALayer()) }).first {
            save(img, f[1])
        } else { print("no layer contents found") }
    case "move": mouse(.mouseMoved, Double(f[1])!, Double(f[2])!)
    case "click":
        mouse(.leftMouseDown, Double(f[1])!, Double(f[2])!); mouse(.leftMouseUp, Double(f[1])!, Double(f[2])!)
    case "dblclick":
        mouse(.leftMouseDown, Double(f[1])!, Double(f[2])!); mouse(.leftMouseUp, Double(f[1])!, Double(f[2])!)
        mouse(.leftMouseDown, Double(f[1])!, Double(f[2])!, clicks: 2); mouse(.leftMouseUp, Double(f[1])!, Double(f[2])!, clicks: 2)
    case "drag":
        let (x0, y0, x1, y1) = (Double(f[1])!, Double(f[2])!, Double(f[3])!, Double(f[4])!)
        mouse(.leftMouseDown, x0, y0)
        for k in 1...10 { let t = Double(k) / 10; mouse(.leftMouseDragged, x0 + (x1 - x0) * t, y0 + (y1 - y0) * t) }
        mouse(.leftMouseUp, x1, y1)
    case "wheel":
        if let w = window, let cg = CGEvent(scrollWheelEvent2Source: nil, units: .line, wheelCount: 1, wheel1: Int32(f[3])!, wheel2: 0, wheel3: 0) {
            cg.location = w.convertPoint(toScreen: point(Double(f[1])!, Double(f[2])!))
            if let e = NSEvent(cgEvent: cg) { w.sendEvent(e) }
        }
    case "param": param(f.dropFirst().joined(separator: " "))
    default: break
    }
    DispatchQueue.main.asyncAfter(deadline: .now() + delay) { run(lines, i + 1) }
}

let desc = AudioComponentDescription(componentType: kAudioUnitType_MusicDevice, componentSubType: fourcc("JV8e"),
                                     componentManufacturer: fourcc("Jovn"), componentFlags: 0, componentFlagsMask: 0)
AVAudioUnit.instantiate(with: desc, options: [.loadInProcess]) { avu, err in
    guard let avu else { print("instantiate failed: \(String(describing: err))"); exit(1) }
    unit = avu
    print("loaded \(avu.auAudioUnit.componentName ?? "?") v\(avu.auAudioUnit.componentVersion)")
    avu.auAudioUnit.requestViewController { vc in
        DispatchQueue.main.async {
            guard let vc else { print("no view controller"); exit(2) }
            let size = vc.preferredContentSize != .zero ? vc.preferredContentSize : vc.view.frame.size
            print("view size \(size)")
            let w = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.titled, .closable], backing: .buffered, defer: false)
            w.contentViewController = vc
            w.setContentSize(size)
            w.center()
            w.acceptsMouseMovedEvents = true
            w.makeKeyAndOrderFront(nil)
            app.activate(ignoringOtherApps: true)
            window = w
            hostView = vc.view
            run(script.split(separator: "\n"), 0)
        }
    }
}
DispatchQueue.main.asyncAfter(deadline: .now() + 60) { print("timeout"); exit(3) }
app.run()
