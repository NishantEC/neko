import AppKit
import SwiftUI

/// A resizable panel on the trailing edge, used in place of SwiftUI's
/// `.inspector`. The inspector is an AppKit split view; dragging its divider
/// while the panel's content re-wraps loops AppKit's constraint pass until it
/// aborts the app. This is a plain stack with a drag handle, so there is no
/// split view to loop. The width is remembered per panel.
struct SidePanel<Panel: View>: ViewModifier {
    let isPresented: Bool
    let key: String
    let range: ClosedRange<CGFloat>
    let ideal: CGFloat
    @ViewBuilder let panel: () -> Panel
    @State private var width: CGFloat?
    @State private var dragStart: CGFloat?
    @State private var hovering = false

    private var current: CGFloat { min(max(width ?? stored ?? ideal, range.lowerBound), range.upperBound) }
    private var stored: CGFloat? {
        let value = UserDefaults.standard.double(forKey: key)
        return value > 0 ? CGFloat(value) : nil
    }

    func body(content: Content) -> some View {
        HStack(spacing: 0) {
            content.frame(maxWidth: .infinity, maxHeight: .infinity)
            if isPresented {
                handle
                panel()
                    .frame(width: current)
                    .frame(maxHeight: .infinity, alignment: .top)
                    .background(Color(nsColor: .controlBackgroundColor).opacity(0.55))
                    .transition(.move(edge: .trailing).combined(with: .opacity))
            }
        }
        // Both pages that use it draw up to the window's top edge.
        .ignoresSafeArea(.container, edges: .top)
    }

    private var handle: some View {
        Rectangle()
            .fill(Color.primary.opacity(hovering || dragStart != nil ? 0.18 : 0.08))
            .frame(width: 1)
            .frame(maxHeight: .infinity)
            .overlay {
                Color.clear.frame(width: 9).contentShape(Rectangle())
                    .onHover { inside in
                        hovering = inside
                        if inside { NSCursor.resizeLeftRight.push() } else { NSCursor.pop() }
                    }
                    .gesture(
                        DragGesture(minimumDistance: 1, coordinateSpace: .global)
                            .onChanged { value in
                                let start = dragStart ?? current
                                dragStart = start
                                width = min(max(start - value.translation.width, range.lowerBound), range.upperBound)
                            }
                            .onEnded { _ in
                                dragStart = nil
                                UserDefaults.standard.set(Double(current), forKey: key)
                            }
                    )
            }
            .accessibilityLabel("Resize panel")
    }
}

extension View {
    func sidePanel<Panel: View>(isPresented: Bool, key: String, range: ClosedRange<CGFloat>, ideal: CGFloat, @ViewBuilder panel: @escaping () -> Panel) -> some View {
        modifier(SidePanel(isPresented: isPresented, key: key, range: range, ideal: ideal, panel: panel))
    }
}
