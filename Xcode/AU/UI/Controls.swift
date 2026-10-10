import Jovian8Core
import SwiftUI

enum Palette {
    static let panel = Color(red: 0.11, green: 0.11, blue: 0.12)
    static let section = Color(red: 0.15, green: 0.15, blue: 0.16)
    static let line = Color.white.opacity(0.55)
    static let text = Color.white.opacity(0.92)
    static let dim = Color.white.opacity(0.5)
    static let orange = Color(red: 0.93, green: 0.45, blue: 0.16)
    static let yellow = Color(red: 0.93, green: 0.76, blue: 0.18)
    static let red = Color(red: 0.84, green: 0.23, blue: 0.18)
    static let blue = Color(red: 0.22, green: 0.48, blue: 0.86)
    static let white = Color(red: 0.92, green: 0.92, blue: 0.9)
}

/// A labelled panel section with the Jupiter's white rule above the title.
struct PanelSection<Content: View>: View {
    let title: String
    @ViewBuilder var content: Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Rectangle().fill(Palette.line).frame(height: 1)
            Text(title.uppercased())
                .font(.system(size: 9, weight: .semibold))
                .tracking(1.2)
                .foregroundStyle(Palette.text)
            HStack(alignment: .top, spacing: 6) { content }
        }
        .padding(.horizontal, 6)
        .padding(.bottom, 6)
    }
}

/// Vertical fader. Drag to change (Option for fine), double-click resets to the default value.
struct Fader: View {
    @ObservedObject var cell: ParamCell
    var label: String
    var height: CGFloat = 104
    @State private var dragStart: Double?
    @State private var hovering = false

    var body: some View {
        VStack(spacing: 3) {
            GeometryReader { geo in
                let h = geo.size.height
                let capH: CGFloat = 12
                let y = (1 - cell.value) * (h - capH)
                ZStack(alignment: .top) {
                    RoundedRectangle(cornerRadius: 2).fill(Color.black).frame(width: 5).frame(maxWidth: .infinity)
                    ForEach(0..<11) { i in
                        Rectangle().fill(Palette.dim.opacity(0.6)).frame(width: 18, height: 0.5)
                            .offset(y: CGFloat(i) / 10 * (h - capH) + capH / 2)
                    }
                    RoundedRectangle(cornerRadius: 2)
                        .fill(LinearGradient(colors: [Color(white: 0.95), Color(white: 0.72)], startPoint: .top, endPoint: .bottom))
                        .overlay(Rectangle().fill(Color.black).frame(height: 1.5))
                        .frame(width: 22, height: capH)
                        .offset(y: y)
                        .shadow(radius: 1, y: 1)
                }
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0)
                    .onChanged { g in
                        if dragStart == nil { dragStart = cell.value }
                        let fine = NSEvent.modifierFlags.contains(.option) ? 0.2 : 1
                        cell.set((dragStart ?? 0) - Double(g.translation.height / (h - capH)) * fine)
                    }
                    .onEnded { _ in dragStart = nil })
                .onTapGesture(count: 2) { cell.set(cell.spec.defaultValue) }
            }
            .frame(width: 30, height: height)
            Text(label).font(.system(size: 8, weight: .medium)).foregroundStyle(Palette.text).lineLimit(1).fixedSize()
            Text(hovering || dragStart != nil ? cell.display : " ")
                .font(.system(size: 8).monospacedDigit()).foregroundStyle(Palette.yellow).lineLimit(1).fixedSize()
        }
        .onHover { hovering = $0 }
        .help("\(cell.spec.name): \(cell.display)")
    }
}

/// One button per item of a discrete parameter, lit when selected (like the Jupiter's switches).
struct Selector: View {
    @ObservedObject var cell: ParamCell
    var labels: [String]? = nil
    var color: Color = Palette.orange
    var vertical = true

    var body: some View {
        let items = labels ?? cell.spec.items
        let layout = vertical ? AnyLayout(VStackLayout(spacing: 3)) : AnyLayout(HStackLayout(spacing: 3))
        layout {
            ForEach(Array(items.enumerated()), id: \.offset) { i, title in
                PanelButton(title: title, lit: cell.index == i, color: color) { cell.set(index: i) }
            }
        }
    }
}

struct Toggle2: View {
    @ObservedObject var cell: ParamCell
    var title: String
    var color: Color = Palette.yellow
    var body: some View { PanelButton(title: title, lit: cell.isOn, color: color) { cell.toggle() } }
}

struct PanelButton: View {
    let title: String
    let lit: Bool
    let color: Color
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                Circle().fill(lit ? Color.red : Color(white: 0.25)).frame(width: 5, height: 5)
                    .shadow(color: lit ? .red : .clear, radius: 3)
                Text(title).font(.system(size: 8, weight: .semibold)).lineLimit(1).fixedSize()
            }
            .padding(.horizontal, 5)
            .frame(minWidth: 38, minHeight: 16)
            .background(RoundedRectangle(cornerRadius: 2).fill(color.opacity(lit ? 1 : 0.55)))
            .foregroundStyle(Color.black.opacity(0.85))
        }
        .buttonStyle(.plain)
        .fixedSize()
    }
}

/// Compact horizontal control for secondary parameters.
struct ParamRow: View {
    @ObservedObject var cell: ParamCell
    var label: String? = nil
    var width: CGFloat = 230

    var body: some View {
        HStack(spacing: 6) {
            Text(label ?? cell.spec.name).font(.system(size: 9)).foregroundStyle(Palette.text)
                .frame(width: width * 0.42, alignment: .leading).lineLimit(1)
            if cell.spec.isDiscrete {
                Picker("", selection: Binding(get: { cell.index }, set: { cell.set(index: $0) })) {
                    ForEach(Array(cell.spec.items.enumerated()), id: \.offset) { i, t in Text(t).tag(i) }
                }
                .labelsHidden().controlSize(.mini).frame(width: width * 0.58)
            } else {
                Slider(value: Binding(get: { cell.value }, set: { cell.set($0) }), in: 0...1)
                    .controlSize(.mini).frame(width: width * 0.36)
                Text(cell.display).font(.system(size: 8).monospacedDigit()).foregroundStyle(Palette.yellow)
                    .frame(width: width * 0.2, alignment: .trailing).lineLimit(1)
            }
        }
        .onTapGesture(count: 2) { cell.set(cell.spec.defaultValue) }
    }
}
