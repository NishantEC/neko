import SwiftUI

// MARK: - Tokens
/// "Quiet craft": neutral ink, hairline structure, monospaced numerics, one
/// accent taken from the mascot's green eyes. Color is reserved for state.
enum NekoStyle {
    static let accent = Color(red: 0.9607843137254902, green: 0.6470588235294118, blue: 0.1411764705882353)
    static let rose = Color(red: 0.96, green: 0.52, blue: 0.66)
    static let pearl = Color(red: 0.975, green: 0.973, blue: 0.965)
    static let plum = Color(red: 0.07, green: 0.07, blue: 0.08)
    static let lilac = Color(red: 0.788235294117647, green: 0.6274509803921569, blue: 1)
    static let mint = Color(red: 0.48627450980392156, green: 0.8392156862745098, blue: 0.6274509803921569)
    static let amber = Color(red: 0.9607843137254902, green: 0.6470588235294118, blue: 0.1411764705882353)
    static let sky = Color(red: 0.43529411764705883, green: 0.6588235294117647, blue: 1)
    static let coral = Color(red: 1, green: 0.4196078431372549, blue: 0.4196078431372549)
    static let signature = LinearGradient(colors: [accent, accent], startPoint: .top, endPoint: .bottom)
    static let radius: CGFloat = 12
    static let radiusSmall: CGFloat = 7
}

enum NekoFont {
    static let display = Font.system(size: 24, weight: .semibold)
    static let title = Font.system(size: 15, weight: .semibold)
    static let heading = Font.system(size: 13, weight: .medium)
    static let body = Font.system(size: 13)
    static let meta = Font.system(size: 11.5)
    static let label = Font.system(size: 10.5, weight: .regular, design: .monospaced)
    static let mono = Font.system(size: 11, weight: .regular, design: .monospaced)
}

struct Ink {
    let scheme: ColorScheme
    var dark: Bool { scheme == .dark }
    var base: Color { dark ? Color(red: 0.043, green: 0.043, blue: 0.047) : NekoStyle.pearl }
    var panel: Color { dark ? Color(red: 0.063, green: 0.063, blue: 0.07) : .white }
    var raised: Color { dark ? Color.white.opacity(0.035) : Color.black.opacity(0.025) }
    var raisedHover: Color { dark ? Color.white.opacity(0.06) : Color.black.opacity(0.045) }
    var line: Color { dark ? Color.white.opacity(0.075) : Color.black.opacity(0.08) }
    var lineStrong: Color { dark ? Color.white.opacity(0.13) : Color.black.opacity(0.14) }
    var faint: Color { dark ? Color.white.opacity(0.38) : Color.black.opacity(0.4) }
}

extension EnvironmentValues { var ink: Ink { Ink(scheme: colorScheme) } }
extension Ink { static var panelColor: Color { Color(nsColor: NSColor(name: nil) { $0.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? NSColor(red: 0.075, green: 0.075, blue: 0.082, alpha: 1) : .white }) } }

// MARK: - Structure
struct HairlineShape: Shape {
    var vertical = false
    func path(in r: CGRect) -> Path {
        Path { p in
            if vertical { p.move(to: .init(x: r.midX, y: r.minY)); p.addLine(to: .init(x: r.midX, y: r.maxY)) }
            else { p.move(to: .init(x: r.minX, y: r.midY)); p.addLine(to: .init(x: r.maxX, y: r.midY)) }
        }
    }
}

struct Hairline: View {
    var dashed = false
    var vertical = false
    @Environment(\.ink) private var ink
    var body: some View {
        HairlineShape(vertical: vertical).stroke(ink.line, style: StrokeStyle(lineWidth: 1, dash: dashed ? [3, 3] : []))
            .frame(maxWidth: vertical ? 1 : .infinity, maxHeight: vertical ? .infinity : 1)
            .frame(width: vertical ? 1 : nil, height: vertical ? nil : 1)
            .accessibilityHidden(true)
    }
}

/// Diagonal hatch gutter, used sparingly to frame content.
struct Hatch: View {
    @Environment(\.ink) private var ink
    var body: some View {
        Canvas { ctx, size in
            var x: CGFloat = -size.height
            while x < size.width {
                var p = Path(); p.move(to: .init(x: x, y: size.height)); p.addLine(to: .init(x: x + size.height, y: 0))
                ctx.stroke(p, with: .color(ink.line), lineWidth: 1); x += 6
            }
        }.accessibilityHidden(true)
    }
}

struct OpalBackground: View {
    @Environment(\.ink) private var ink
    var expressive = false
    var body: some View { (expressive ? ink.base : ink.panel).ignoresSafeArea().allowsHitTesting(false).accessibilityHidden(true) }
}

struct NoiseOverlay: View { var body: some View { Color.clear } }

// MARK: - Surfaces
@MainActor struct NekoCardModifier: ViewModifier {
    var padding: CGFloat = 16
    var radius: CGFloat = NekoStyle.radius
    var highlighted = false
    var interactive = false
    @Environment(\.ink) private var ink
    @State private var hover = false
    func body(content: Content) -> some View {
        content.padding(padding)
            .background(hover && interactive ? ink.raisedHover : ink.raised, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(highlighted ? NekoStyle.accent.opacity(0.55) : (hover && interactive ? ink.lineStrong : ink.line)))
            .animation(.easeOut(duration: 0.14), value: hover)
            .onHover { if interactive { hover = $0 } }
    }
}

extension View {
    func nekoCard(padding: CGFloat = 16, radius: CGFloat = NekoStyle.radius, highlighted: Bool = false, interactive: Bool = false) -> some View {
        modifier(NekoCardModifier(padding: padding, radius: radius, highlighted: highlighted, interactive: interactive))
    }
}

// MARK: - Buttons
struct NekoPrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var enabled
    @Environment(\.colorScheme) private var scheme
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.font(.system(size: 12.5, weight: .medium))
            .foregroundStyle(scheme == .dark ? Color.black : .white)
            .padding(.horizontal, 12).padding(.vertical, 6)
            .background(scheme == .dark ? Color.white.opacity(configuration.isPressed ? 0.8 : 0.94) : Color.black.opacity(configuration.isPressed ? 0.75 : 0.9), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .opacity(enabled ? 1 : 0.35)
            .scaleEffect(configuration.isPressed ? 0.98 : 1)
            .animation(.easeOut(duration: 0.1), value: configuration.isPressed)
    }
}

@MainActor struct NekoGhostButtonStyle: ButtonStyle {
    @Environment(\.ink) private var ink
    @State private var hover = false
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.font(.system(size: 12.5))
            .padding(.horizontal, 11).padding(.vertical, 6)
            .background(configuration.isPressed ? ink.raisedHover : (hover ? ink.raised : .clear), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 8, style: .continuous).strokeBorder(hover ? ink.lineStrong : ink.line))
            .animation(.easeOut(duration: 0.12), value: hover)
            .onHover { hover = $0 }
    }
}

extension View {
    func nekoGlassButton() -> some View { buttonStyle(NekoGhostButtonStyle()) }
    func nekoPrimaryButton() -> some View { buttonStyle(NekoPrimaryButtonStyle()) }
}

struct GlassActions<Content: View>: View {
    @ViewBuilder var content: () -> Content
    var body: some View { content() }
}

// MARK: - Small components
/// Mono uppercase label with a right-aligned zero-padded count.
struct SectionLabel: View {
    let text: String
    var count: Int? = nil
    var index: Int? = nil
    var body: some View {
        HStack(spacing: 6) {
            Text(text).foregroundStyle(.secondary)
            if let count, count > 0 { Text(String(count)).foregroundStyle(.tertiary).monospacedDigit() }
            Spacer(minLength: 0)
        }.font(.system(size: 12, weight: .medium)).accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
    }
}

struct Chip: View {
    let text: String
    var icon: String? = nil
    var tint: Color = .secondary
    @Environment(\.ink) private var ink
    var body: some View {
        HStack(spacing: 5) {
            if let icon { Image(systemName: icon).font(.system(size: 9)).foregroundStyle(tint) }
            Text(text).lineLimit(1).foregroundStyle(.secondary)
        }.font(.system(size: 11))
            .padding(.horizontal, 7).padding(.vertical, 2.5)
            .overlay(RoundedRectangle(cornerRadius: 5, style: .continuous).strokeBorder(ink.line))
    }
}

struct KeyCap: View {
    let key: String
    @Environment(\.ink) private var ink
    var body: some View {
        Text(key).font(NekoFont.mono).foregroundStyle(.secondary)
            .padding(.horizontal, 5).padding(.vertical, 1.5)
            .background(ink.raised, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 4, style: .continuous).strokeBorder(ink.line))
    }
}

/// Linear-style status ring: fills as work advances, pulses while running.
struct StatusGlyph: View {
    let status: String
    var size: CGFloat = 14
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var pulse = false
    var color: Color { taskColor(status) }
    var progress: Double {
        switch status { case "Queued": 0; case "Planning": 0.25; case "Building": 0.5; case "Reviewing": 0.75; case "AwaitingApproval": 0.5; case "ReadyForReview": 0.5; default: 1 }
    }
    var body: some View {
        ZStack {
            switch status {
            case "Completed":
                Circle().fill(color); Image(systemName: "checkmark").font(.system(size: size * 0.5, weight: .bold)).foregroundStyle(.black)
            case "Cancelled":
                Circle().strokeBorder(Color.secondary, lineWidth: 1.4); Image(systemName: "xmark").font(.system(size: size * 0.42, weight: .bold)).foregroundStyle(.secondary)
            case "Failed":
                Circle().fill(color); Image(systemName: "exclamationmark").font(.system(size: size * 0.5, weight: .bold)).foregroundStyle(.black)
            default:
                Circle().strokeBorder(color, style: StrokeStyle(lineWidth: 1.4, dash: status == "Queued" ? [1.6, 1.6] : []))
                Circle().trim(from: 0, to: progress).stroke(color, style: StrokeStyle(lineWidth: size * 0.36)).rotationEffect(.degrees(-90)).padding(size * 0.3)
            }
        }.frame(width: size, height: size)
            .opacity(isLive && pulse ? 0.55 : 1)
            .onAppear { if isLive && !reduceMotion { withAnimation(.easeInOut(duration: 1.1).repeatForever()) { pulse = true } } }
            .accessibilityLabel(friendlyTaskStatus(status))
    }
    private var isLive: Bool { ["Planning", "Building", "Reviewing"].contains(status) }
}

func taskColor(_ status: String) -> Color {
    switch status {
    case "AwaitingApproval": NekoStyle.amber
    case "ReadyForReview": NekoStyle.sky
    case "Completed": NekoStyle.mint
    case "Failed": NekoStyle.coral
    case "Cancelled": .secondary
    default: NekoStyle.lilac
    }
}

struct Avatar: View {
    let role: String
    var size: CGFloat = 22
    @Environment(\.ink) private var ink
    var body: some View {
        if role == "user" {
            Circle().fill(ink.raisedHover).overlay(Circle().strokeBorder(ink.line))
                .overlay(Text("N").font(.system(size: size * 0.45, weight: .medium)).foregroundStyle(.secondary))
                .frame(width: size, height: size).accessibilityHidden(true)
        } else {
            BrandMark(size: size).clipShape(RoundedRectangle(cornerRadius: size * 0.26, style: .continuous))
        }
    }
}

struct EmptyState<Action: View>: View {
    let title: String
    let message: String
    var icon: String = "sparkles"
    @ViewBuilder var action: () -> Action
    @Environment(\.ink) private var ink
    var body: some View {
        VStack(spacing: 10) {
            BrandMark(size: 44).clipShape(RoundedRectangle(cornerRadius: 11, style: .continuous)).opacity(0.9)
            Text(title).font(NekoFont.title)
            Text(message).font(NekoFont.body).foregroundStyle(.secondary).multilineTextAlignment(.center).frame(maxWidth: 340)
            action().padding(.top, 4)
        }.padding(36).frame(maxWidth: .infinity)
    }
}
extension EmptyState where Action == EmptyView {
    init(title: String, message: String, icon: String = "sparkles") { self.init(title: title, message: message, icon: icon) { EmptyView() } }
}

struct PageHeader<Trailing: View>: View {
    let title: String
    var subtitle: String? = nil
    @ViewBuilder var trailing: () -> Trailing
    var body: some View {
        VStack(spacing: 0) {
            HStack(alignment: .lastTextBaseline) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(title).font(NekoFont.title)
                    if let subtitle { Text(subtitle).font(NekoFont.body).foregroundStyle(.secondary) }
                }
                Spacer()
                trailing()
            }.padding(.horizontal, 28).padding(.top, 20).padding(.bottom, 18)
            Hairline(dashed: true)
        }
    }
}

// MARK: - Greeting
/// Editorial greeting: serif headline, mono date line. No decorative field.
struct OpalGreeting: View {
    var title: String? = nil
    var subtitle = "Tell me what needs moving forward. We’ll take it one step at a time."
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var appeared = false
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text(Date.now.formatted(.dateTime.weekday(.wide).day().month(.wide))).font(.system(size: 12)).foregroundStyle(.tertiary)
            Text(title ?? "\(greeting). What should we move forward?").font(NekoFont.display).tracking(-0.4).lineSpacing(2).fixedSize(horizontal: false, vertical: true)
            Text(subtitle).font(.system(size: 13)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .opacity(appeared || reduceMotion ? 1 : 0).offset(y: appeared || reduceMotion ? 0 : 6)
        .onAppear { withAnimation(.easeOut(duration: 0.45)) { appeared = true } }
    }
    private var greeting: String {
        let h = Calendar.current.component(.hour, from: Date())
        return h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
    }
}

struct TypingDots: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var phase = false
    var body: some View {
        HStack(spacing: 3) {
            ForEach(0..<3) { i in
                Circle().fill(Color.secondary).frame(width: 4, height: 4)
                    .opacity(phase ? 1 : 0.25)
                    .animation(reduceMotion ? nil : .easeInOut(duration: 0.5).repeatForever().delay(Double(i) * 0.15), value: phase)
            }
        }.onAppear { phase = true }.accessibilityLabel("Neko is thinking")
    }
}

func friendlyTaskStatus(_ status: String) -> String {
    switch status {
    case "AwaitingApproval": "Needs approval"
    case "ReadyForReview": "Ready to review"
    case "Queued": "Queued"
    case "Planning": "Making a plan"
    case "Building": "Preparing a fix"
    case "Reviewing": "Checking the result"
    case "Completed": "Completed"
    case "Cancelled": "Cancelled"
    case "Failed": "Needs attention"
    default: status
    }
}

/// HiFi v2 tokens (Paper "Native · HiFi v2"). Night-shift instrument panel:
/// neutral instrument surfaces, one amber indicator. 4pt spacing grid.
enum N {
    static let canvas = Color(red: 0.043137254901960784, green: 0.043137254901960784, blue: 0.047058823529411764)
    static let panel = Color(red: 0.07450980392156863, green: 0.07450980392156863, blue: 0.08235294117647059)
    static let card = Color(red: 0.09411764705882353, green: 0.09411764705882353, blue: 0.10588235294117647)
    static let selected = Color(red: 0.11764705882352941, green: 0.11764705882352941, blue: 0.12941176470588237)
    static let line = Color.white.opacity(0.06)
    static let lineStrong = Color.white.opacity(0.09)
    static let text = Color(red: 0.9294117647058824, green: 0.9294117647058824, blue: 0.9372549019607843)
    static let text2 = Color(red: 0.6313725490196078, green: 0.6313725490196078, blue: 0.6509803921568628)
    static let text3 = Color(red: 0.5568627450980392, green: 0.5568627450980392, blue: 0.5803921568627451)
    static let text4 = Color(red: 0.43137254901960786, green: 0.43137254901960786, blue: 0.4549019607843137)
    static let sidebarWidth: CGFloat = 232
    static let headerHeight: CGFloat = 48
    static let rowHeight: CGFloat = 30
}

/// Retired: the system toolbar now owns title and subtitle. Kept as a no-op
/// so existing screens compile unchanged.
struct PanelHeader<Trailing: View>: View {
    let title: String
    var crumb: String? = nil
    @ViewBuilder var trailing: () -> Trailing
    var body: some View { EmptyView() }
}

struct OutlineTag: View {
    let text: String
    var body: some View {
        Text(text).font(.system(size: 12)).foregroundStyle(N.text3)
            .padding(.horizontal, 10).padding(.vertical, 4)
            .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).strokeBorder(Color.white.opacity(0.08)))
    }
}

// MARK: - Liquid Glass (macOS 26), with material fallbacks

/// Makes the hosting NSWindow non-opaque so behind-window glass can show the desktop.
struct TranslucentWindow: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        let view = NSView()
        DispatchQueue.main.async {
            guard let window = view.window else { return }
            window.isOpaque = false
            window.backgroundColor = .clear
            window.titlebarAppearsTransparent = true
        }
        return view
    }
    func updateNSView(_ nsView: NSView, context: Context) {}
}

struct BehindWindowGlass: NSViewRepresentable {
    var material: NSVisualEffectView.Material = .underWindowBackground
    func makeNSView(context: Context) -> NSVisualEffectView {
        let v = NSVisualEffectView()
        v.material = material; v.blendingMode = .behindWindow; v.state = .active
        return v
    }
    func updateNSView(_ v: NSVisualEffectView, context: Context) { v.material = material }
}

/// Window canvas: real desktop-refracting glass, tinted toward instrument black.
struct GlassCanvas: View {
    @Environment(\.accessibilityReduceTransparency) private var opaque
    var body: some View {
        ZStack {
            if opaque { N.canvas } else {
                BehindWindowGlass(material: .underWindowBackground)
                N.canvas.opacity(0.42)
                // A single warm ambient glow keeps the glass from reading as flat grey.
                RadialGradient(colors: [NekoStyle.accent.opacity(0.10), .clear], center: .topLeading, startRadius: 0, endRadius: 520)
            }
        }.ignoresSafeArea().allowsHitTesting(false).accessibilityHidden(true)
    }
}

extension View {
    /// Liquid Glass surface; falls back to a material on older systems.
    @ViewBuilder func liquidGlass(radius: CGFloat, interactive: Bool = false, tint: Color? = nil) -> some View {
        if #available(macOS 26, *) {
            let base: Glass = tint.map { Glass.regular.tint($0) } ?? .regular
            self.glassEffect(interactive ? base.interactive() : base, in: .rect(cornerRadius: radius))
        } else {
            self.background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
        }
    }
    @ViewBuilder func liquidGlassCapsule(interactive: Bool = false) -> some View {
        if #available(macOS 26, *) { self.glassEffect(interactive ? .regular.interactive() : .regular, in: .capsule) }
        else { self.background(.ultraThinMaterial, in: Capsule()) }
    }
    @ViewBuilder func glassMorphID(_ id: String, in ns: Namespace.ID) -> some View {
        if #available(macOS 26, *) { self.glassEffectID(id, in: ns) } else { self }
    }
    @ViewBuilder func glassButton() -> some View {
        if #available(macOS 26, *) { self.buttonStyle(.glass) } else { self.buttonStyle(.bordered) }
    }
}

struct GlassGroup<Content: View>: View {
    var spacing: CGFloat = 8
    @ViewBuilder var content: () -> Content
    var body: some View {
        if #available(macOS 26, *) { GlassEffectContainer(spacing: spacing) { content() } } else { content() }
    }
}

struct StatusPill: View {
    let text: String
    var live = true
    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(live ? NekoStyle.accent : N.text4).frame(width: 6, height: 6)
                .shadow(color: NekoStyle.accent.opacity(live ? 0.8 : 0), radius: 4)
            Text(text).font(.system(size: 12, weight: .medium)).foregroundStyle(N.text2)
        }.padding(.horizontal, 10).frame(height: 26).liquidGlassCapsule()
    }
}



// MARK: - Looks (selectable variants)

enum NekoLook: String, CaseIterable, Identifiable {
    case system, ambient, dense, mascot
    var id: String { rawValue }
    var title: String {
        switch self {
        case .system: "System — pure native glass"
        case .ambient: "Ambient — colour under the glass"
        case .dense: "Dense — Linear-style rows"
        case .mascot: "Mascot — expressive Today"
        }
    }
}

private struct NekoLookKey: EnvironmentKey { static let defaultValue: NekoLook = .ambient }
extension EnvironmentValues {
    var nekoLook: NekoLook {
        get { self[NekoLookKey.self] }
        set { self[NekoLookKey.self] = newValue }
    }
}

/// Content-layer backdrop. Glass needs something to refract, so the ambient and
/// mascot looks paint a soft field that extends under the sidebar glass.
struct LookBackground: View {
    let look: String
    @Environment(\.accessibilityReduceTransparency) private var opaque
    var body: some View {
        switch NekoLook(rawValue: look) ?? .ambient {
        case .system, .dense:
            Color.clear
        case .ambient, .mascot:
            if opaque { Color(nsColor: .windowBackgroundColor) } else { field.extendsUnderSidebar() }
        }
    }
    @ViewBuilder private var field: some View {
        if #available(macOS 15, *) {
            MeshGradient(width: 3, height: 3,
                points: [.init(0, 0), .init(0.5, 0), .init(1, 0), .init(0, 0.5), .init(0.45, 0.55), .init(1, 0.5), .init(0, 1), .init(0.5, 1), .init(1, 1)],
                colors: [
                    Color(red: 0.16, green: 0.10, blue: 0.05), Color(red: 0.07, green: 0.07, blue: 0.09), Color(red: 0.08, green: 0.06, blue: 0.14),
                    Color(red: 0.28, green: 0.16, blue: 0.05), Color(red: 0.08, green: 0.08, blue: 0.10), Color(red: 0.14, green: 0.08, blue: 0.20),
                    Color(red: 0.07, green: 0.07, blue: 0.08), Color(red: 0.06, green: 0.06, blue: 0.07), Color(red: 0.09, green: 0.07, blue: 0.12)
                ])
        } else {
            LinearGradient(colors: [Color(red: 0.2, green: 0.12, blue: 0.05), Color(red: 0.07, green: 0.07, blue: 0.09)], startPoint: .topLeading, endPoint: .bottomTrailing)
        }
    }
}

extension View {
    @ViewBuilder func extendsUnderSidebar() -> some View {
        if #available(macOS 26, *) { self.backgroundExtensionEffect() } else { self }
    }
    @ViewBuilder func softScrollEdges() -> some View {
        if #available(macOS 26, *) { self.scrollEdgeEffectStyle(.soft, for: .all) } else { self }
    }
}

extension View {
    @ViewBuilder func glassProminentButton() -> some View {
        if #available(macOS 26, *) { self.buttonStyle(.glassProminent) } else { self.buttonStyle(.borderedProminent) }
    }
}
