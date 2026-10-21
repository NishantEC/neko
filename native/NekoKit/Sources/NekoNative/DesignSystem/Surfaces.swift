import SwiftUI

// MARK: - Glass

/// The one floating-control surface: Liquid Glass on macOS 26, a material
/// below it, and an opaque card with a visible border under Reduce
/// Transparency or Increased Contrast. Only the background changes, so a live
/// editor inside keeps its identity and selection.
struct NekoGlassSurface<S: InsettableShape>: ViewModifier {
    let shape: S
    var interactive = false
    var tint: Color? = nil
    @Environment(\.accessibilityReduceTransparency) private var opaque
    @Environment(\.colorSchemeContrast) private var contrast
    func body(content: Content) -> some View {
        content.background {
            if opaque || contrast == .increased {
                shape.fill(N.card).overlay(shape.strokeBorder(N.lineStrong, lineWidth: contrast == .increased ? 2 : 1))
            } else if #available(macOS 26, *) {
                let base: Glass = tint.map { Glass.regular.tint($0) } ?? .regular
                Color.clear.glassEffect(interactive ? base.interactive() : base, in: shape)
            } else {
                shape.fill(.regularMaterial).overlay(shape.strokeBorder(N.line))
            }
        }
    }
}

extension View {
    /// A floating control surface with rounded corners. See `NekoGlassSurface`.
    func liquidGlass(radius: CGFloat, interactive: Bool = false, tint: Color? = nil) -> some View {
        modifier(NekoGlassSurface(shape: RoundedRectangle(cornerRadius: radius, style: .continuous), interactive: interactive, tint: tint))
    }
    func liquidGlassCapsule(interactive: Bool = false) -> some View {
        modifier(NekoGlassSurface(shape: Capsule(), interactive: interactive))
    }
    @ViewBuilder func glassMorphID(_ id: String, in ns: Namespace.ID) -> some View {
        if #available(macOS 26, *) { self.glassEffectID(id, in: ns) } else { self }
    }
}

/// Lets neighbouring glass surfaces merge and morph on macOS 26.
struct GlassGroup<Content: View>: View {
    var spacing: CGFloat = NekoSpace.sm
    @ViewBuilder var content: () -> Content
    var body: some View {
        if #available(macOS 26, *) { GlassEffectContainer(spacing: spacing) { content() } } else { content() }
    }
}

// MARK: - Opaque surfaces

/// How far a surface sits above the canvas. Elevation comes from light, not
/// lines: borders give structure, shadows give height.
enum NekoElevation {
    /// Cards and grouped sections on a page.
    case raised
    /// Menus, popovers and previews that float over other content.
    case floating
}

/// Shadows for an elevation. Light mode only for `raised` (a dark canvas has
/// no light to cast them; `NekoRaisedSurface` gives dark mode a lit edge
/// instead), and none for `raised` under Increased Contrast, which draws a
/// strong border. `lifted` is the hover state of an interactive card.
private struct NekoElevationModifier: ViewModifier {
    let level: NekoElevation
    var lifted = false
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    func body(content: Content) -> some View {
        switch level {
        case .raised:
            // Zero-opacity shadows rather than none, so hover and appearance
            // changes animate the same two layers instead of swapping views.
            let shows = scheme == .light && contrast != .increased
            content
                .shadow(color: .black.opacity(shows ? 0.06 : 0), radius: 1, y: 1)
                .shadow(color: .black.opacity(shows ? (lifted ? 0.08 : 0.05) : 0), radius: lifted ? 16 : 12, y: lifted ? 8 : 6)
        case .floating:
            content.shadow(color: .black.opacity(0.18), radius: 24, y: 12)
        }
    }
}

extension View {
    /// The shadow for an elevation. Apply it to an opaque shape or a view
    /// with an opaque background, or the shadow follows every glyph.
    func nekoElevation(_ level: NekoElevation, lifted: Bool = false) -> some View {
        modifier(NekoElevationModifier(level: level, lifted: lifted))
    }
}

/// The raised, opaque surface behind cards and grouped sections: no border by
/// default, a soft shadow in light mode and a lit top edge in dark mode.
/// Increased Contrast swaps both for a 2pt strong border. `highlighted` marks
/// the selected or current item with the accent.
struct NekoRaisedSurface: View {
    var radius: CGFloat = NekoRadius.xl
    var highlighted = false
    var lifted = false
    @Environment(\.colorScheme) private var scheme
    @Environment(\.colorSchemeContrast) private var contrast
    var body: some View {
        let shape = RoundedRectangle(cornerRadius: radius, style: .continuous)
        // A hover wash over the card rather than in place of it, so the
        // surface never turns transparent.
        shape.fill(N.card).overlay { shape.fill(N.selected).opacity(lifted ? 1 : 0) }
            .nekoElevation(.raised, lifted: lifted)
            .overlay {
                if highlighted {
                    shape.strokeBorder(NekoStyle.accent, lineWidth: contrast == .increased ? 2 : 1.5)
                } else if contrast == .increased {
                    shape.strokeBorder(N.lineStrong, lineWidth: 2)
                } else if scheme == .dark {
                    shape.strokeBorder(LinearGradient(colors: [.white.opacity(0.08), .white.opacity(0.02)], startPoint: .top, endPoint: .bottom), lineWidth: 1)
                }
            }
    }
}

/// A raised, opaque content surface. `interactive` adds a hover wash and lift.
struct NekoCardModifier: ViewModifier {
    var padding: CGFloat = NekoSpace.xl
    var radius: CGFloat = NekoRadius.xl
    var highlighted = false
    var interactive = false
    @State private var hover = false
    func body(content: Content) -> some View {
        content.padding(padding)
            .background { NekoRaisedSurface(radius: radius, highlighted: highlighted, lifted: hover && interactive) }
            .nekoAnimation(.hover, value: hover)
            .onHover { if interactive { hover = $0 } }
    }
}

extension View {
    func nekoCard(padding: CGFloat = NekoSpace.xl, radius: CGFloat = NekoRadius.xl, highlighted: Bool = false, interactive: Bool = false) -> some View {
        modifier(NekoCardModifier(padding: padding, radius: radius, highlighted: highlighted, interactive: interactive))
    }
    /// A filled rounded background with an optional hairline, the shape every
    /// small block (code, attachment, callout) shares.
    func nekoFill(_ color: Color, radius: CGFloat, stroke: Color? = nil) -> some View {
        background(color, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay { if let stroke { RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(stroke) } }
    }
}

// MARK: - Lines

struct HairlineShape: Shape {
    var vertical = false
    func path(in r: CGRect) -> Path {
        Path { p in
            if vertical { p.move(to: .init(x: r.midX, y: r.minY)); p.addLine(to: .init(x: r.midX, y: r.maxY)) }
            else { p.move(to: .init(x: r.minX, y: r.midY)); p.addLine(to: .init(x: r.maxX, y: r.midY)) }
        }
    }
}

/// A one-point rule that strengthens under Increased Contrast.
struct Hairline: View {
    var dashed = false
    var vertical = false
    @Environment(\.colorSchemeContrast) private var contrast
    var body: some View {
        HairlineShape(vertical: vertical).stroke(contrast == .increased ? N.lineStrong : N.line, style: StrokeStyle(lineWidth: 1, dash: dashed ? [3, 3] : []))
            .frame(maxWidth: vertical ? 1 : .infinity, maxHeight: vertical ? .infinity : 1)
            .frame(width: vertical ? 1 : nil, height: vertical ? nil : 1)
            .accessibilityHidden(true)
    }
}

// MARK: - Window and scroll behaviour

extension View {
    /// Split-view panes must report a size range that doesn't change with
    /// their content. Text that rewraps as a divider moves (a ticket thread,
    /// a growing reply box) otherwise changes the pane's minimum size during
    /// AppKit's constraint pass, which loops until AppKit aborts the app.
    /// The column width limits still apply.
    func stableSplitPane() -> some View {
        frame(minWidth: 0, maxWidth: .infinity, minHeight: 0, maxHeight: .infinity, alignment: .top)
    }
    @ViewBuilder func softScrollEdges() -> some View {
        if #available(macOS 26, *) { self.scrollEdgeEffectStyle(.soft, for: .all) } else { self }
    }
    @ViewBuilder func hiddenWindowToolbarBackground() -> some View {
        if #available(macOS 15, *) { self.toolbarBackgroundVisibility(.hidden, for: .windowToolbar) } else { self }
    }
}

extension ToolbarContent {
    /// Custom search and segmented controls already draw their own surfaces.
    @ToolbarContentBuilder func withoutSharedBackground() -> some ToolbarContent {
        if #available(macOS 26, *) { sharedBackgroundVisibility(.hidden) } else { self }
    }
}
