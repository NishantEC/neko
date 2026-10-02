import SwiftUI

/// Neko's segmented control: a plain capsule track with a Liquid Glass thumb
/// that flows to the chosen option. On macOS 26 the thumb is real glass that
/// morphs between options; earlier systems get a sliding material thumb.
struct GlassSegmented<Value: Hashable>: View {
    struct Option: Identifiable {
        let value: Value
        let title: String
        var symbol: String? = nil
        var help: String? = nil
        var id: String { title }
    }
    @Binding var selection: Value
    let options: [Option]
    /// Show only each option's symbol, with its title as tooltip and label.
    var iconOnly = false
    var size: ControlSize = .regular
    @Namespace private var thumbSpace
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var height: CGFloat { size == .small ? 22 : 26 }

    var body: some View {
        container {
            HStack(spacing: 2) {
                ForEach(Array(options.enumerated()), id: \.element.id) { index, option in
                    segment(option, index: index)
                }
            }
            .padding(2)
            .background(Capsule().fill(Color.white.opacity(0.06)))
            .overlay(Capsule().strokeBorder(Color.white.opacity(0.09)))
        }
        .fixedSize()
        .accessibilityElement(children: .contain)
    }

    @ViewBuilder private func container<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        if #available(macOS 26, *) {
            // Spacing is the merge distance: adjacent thumbs blend into one flow.
            GlassEffectContainer(spacing: 24) { content() }
        } else {
            content()
        }
    }

    private func segment(_ option: Option, index: Int) -> some View {
        let selected = option.value == selection
        return Button {
            guard !selected else { return }
            withAnimation(reduceMotion ? nil : .smooth(duration: 0.34, extraBounce: 0.12)) { selection = option.value }
        } label: {
            Group {
                if iconOnly, let symbol = option.symbol {
                    Image(systemName: symbol).font(.system(size: size == .small ? 11 : 12, weight: .medium))
                        .frame(width: size == .small ? 26 : 30)
                } else {
                    HStack(spacing: 5) {
                        if let symbol = option.symbol { Image(systemName: symbol).font(.system(size: 11, weight: .medium)) }
                        Text(option.title).font(.system(size: size == .small ? 12 : 13, weight: selected ? .semibold : .regular))
                    }
                    .padding(.horizontal, size == .small ? 10 : 12)
                }
            }
            .foregroundStyle(selected ? Color.primary : Color.secondary)
            .frame(height: height)
            .background { thumb(selected, index: index) }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help(option.help ?? option.title)
        .accessibilityLabel(option.title)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }

    /// Present only under the chosen option, so changing selection removes one
    /// glass shape and inserts another: the system morphs between them.
    @ViewBuilder private func thumb(_ selected: Bool, index: Int) -> some View {
        if selected {
            if #available(macOS 26, *) {
                Color.clear
                    .glassEffect(.regular.interactive(), in: .capsule)
                    .glassEffectID("thumb-\(index)", in: thumbSpace)
            } else {
                Capsule().fill(.regularMaterial)
                    .overlay(Capsule().strokeBorder(Color.white.opacity(0.14)))
                    .matchedGeometryEffect(id: "thumb", in: thumbSpace)
            }
        }
    }
}
