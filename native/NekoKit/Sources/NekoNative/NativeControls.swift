import SwiftUI

/// Shared geometry for search, segmented controls and their adjacent actions.
enum NekoControlMetrics {
    static let segmentInset: CGFloat = 2
    static func segmentHeight(for size: ControlSize) -> CGFloat { size == .small ? 22 : 26 }
    static func height(for size: ControlSize = .regular) -> CGFloat {
        segmentHeight(for: size) + segmentInset * 2
    }
}

/// A native text field with the same visible height as Neko's segmented controls.
struct NekoSearchField: View {
    let title: String
    @Binding var text: String
    var size: ControlSize = .regular
    @FocusState private var focused: Bool

    var body: some View {
        TextField(title, text: $text)
            .textFieldStyle(.plain)
            .font(.system(size: size == .small ? 12 : 13))
            .controlSize(size)
            .focused($focused)
            .padding(.horizontal, 10)
            .frame(height: NekoControlMetrics.height(for: size))
            .background(Color(nsColor: .textBackgroundColor), in: RoundedRectangle(cornerRadius: 8))
            .overlay {
                RoundedRectangle(cornerRadius: 8).strokeBorder(
                    focused ? Color.accentColor : Color.primary.opacity(0.12),
                    lineWidth: focused ? 2 : 1
                ).allowsHitTesting(false)
            }
            .contentShape(Rectangle())
            .simultaneousGesture(TapGesture().onEnded { focused = true })
            .accessibilityLabel(title)
    }
}
