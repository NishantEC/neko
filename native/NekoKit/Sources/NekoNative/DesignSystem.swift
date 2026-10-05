import SwiftUI

// MARK: - Tokens
/// Shared native chrome. Colour marks actions and state; content stays neutral.
enum NekoStyle {
    // One meaning per colour. OKLCH values; see Palette section below.
    static let accent = Color(nsColor: .systemBlue)
    static let amber = Color.adaptive(light: 0x895600, dark: 0xE8BB68) // needs approval
    static let sky = Color.adaptive(light: 0x1765A9, dark: 0x8FC4EF)   // ready to review
    static let mint = Color.adaptive(light: 0x247A50, dark: 0x83D3AC)  // done
    static let coral = Color.adaptive(light: 0xB23838, dark: 0xF19E98) // failed
    static let lilac = Color.oklch(0.74, 0.09, 300)      // workspace tag only
    static let rose = Color.oklch(0.74, 0.09, 350)       // workspace tag only
    static let pearl = Color.oklch(0.975, 0.003, 264)
    static let plum = Color.oklch(0.16, 0.004, 264)
    static let signature = LinearGradient(colors: [accent, accent], startPoint: .top, endPoint: .bottom)
    static let radius: CGFloat = 18
    static let radiusSmall: CGFloat = 10
}

enum NekoFont {
    static let display = Font.system(size: 28, weight: .semibold)
    static let title = Font.system(size: 20, weight: .semibold)
    static let heading = Font.system(size: 15, weight: .semibold)
    static let body = Font.system(size: 15)
    static let chat = Font.system(size: 15)
    static let meta = Font.system(size: 13)
    static let label = Font.system(size: 11, weight: .medium)
    static let mono = Font.system(size: 12, weight: .regular, design: .monospaced)
}

/// Bounded content keeps paragraphs and controls in the same visual lanes.
/// Board columns and native window chrome intentionally fill their container.
enum NekoLayout {
    static let pageWidth: CGFloat = 880
    static let readingWidth: CGFloat = 760
    static let pageInset: CGFloat = 32
    static let sectionGap: CGFloat = 28
    static let rowInset: CGFloat = 18
}

struct Ink {
    let scheme: ColorScheme
    var dark: Bool { scheme == .dark }
    var base: Color { N.canvas }
    var panel: Color { N.panel }
    var raised: Color { N.card }
    var raisedHover: Color { N.selected }
    var line: Color { N.line }
    var lineStrong: Color { N.lineStrong }
    var faint: Color { N.text4 }
}

extension EnvironmentValues { var ink: Ink { Ink(scheme: colorScheme) } }
extension Ink { static var panelColor: Color { N.panel } }

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
    var padding: CGFloat = 20
    var radius: CGFloat = NekoStyle.radius
    var highlighted = false
    var interactive = false
    @Environment(\.ink) private var ink
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var hover = false
    func body(content: Content) -> some View {
        content.padding(padding)
            .background(hover && interactive ? ink.raisedHover : ink.raised, in: RoundedRectangle(cornerRadius: radius, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: radius, style: .continuous).strokeBorder(highlighted ? NekoStyle.accent.opacity(0.55) : (hover && interactive ? ink.lineStrong : ink.line)))
            .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: hover)
            .onHover { if interactive { hover = $0 } }
    }
}

extension View {
    /// Split-view panes must report a size range that doesn't change with
    /// their content. Text that rewraps as a divider moves (a ticket thread,
    /// a growing reply box) otherwise changes the pane's minimum size during
    /// AppKit's constraint pass, which loops until AppKit aborts the app.
    /// The column width limits still apply.
    func stableSplitPane() -> some View {
        frame(minWidth: 0, maxWidth: .infinity, minHeight: 0, maxHeight: .infinity, alignment: .top)
    }
    /// A functional notice above page content, using the platform material.
    func nekoToast() -> some View {
        self
            .padding(.horizontal, 18).padding(.vertical, 14)
            .liquidGlass(radius: 18)
            .frame(maxWidth: NekoLayout.readingWidth)
            .padding(.horizontal, 24).padding(.top, 8)
            .frame(maxWidth: .infinity)
    }
    func nekoCard(padding: CGFloat = 20, radius: CGFloat = NekoStyle.radius, highlighted: Bool = false, interactive: Bool = false) -> some View {
        modifier(NekoCardModifier(padding: padding, radius: radius, highlighted: highlighted, interactive: interactive))
    }
}

// MARK: - Buttons
extension View {
    func nekoGlassButton() -> some View { buttonStyle(.bordered) }
    func nekoPrimaryButton() -> some View { buttonStyle(.borderedProminent) }
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
    default: N.text3
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
        VStack(spacing: 12) {
            Image(systemName: icon).font(.system(size: 28, weight: .light))
                .foregroundStyle(.secondary).padding(.bottom, 4).accessibilityHidden(true)
            Text(title).font(NekoFont.title)
            Text(message).font(NekoFont.body).foregroundStyle(.secondary).multilineTextAlignment(.center).lineSpacing(3).frame(maxWidth: 360)
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

/// One neutral surface ramp for the native app, shared by legacy Ink consumers.
/// Small secondary text remains readable on the brightest content surface.
enum N {
    static let canvas = Color.adaptive(light: 0xF7F8FA, dark: 0x1C1C1E)
    static let panel = Color.adaptive(light: 0xFFFFFF, dark: 0x242426)
    static let card = Color.adaptive(light: 0xFFFFFF, dark: 0x2C2C2E)
    static let selected = Color.primary.opacity(0.065)
    static let line = Color.primary.opacity(0.08)
    static let lineStrong = Color.primary.opacity(0.16)
    static let text = Color(nsColor: .labelColor)
    static let text2 = Color.adaptive(light: 0x424954, dark: 0xD7DBE2)
    static let text3 = Color.adaptive(light: 0x606875, dark: 0xB9C0CA)
    static let text4 = Color.adaptive(light: 0x666E7A, dark: 0xA8B0BC)
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
            .overlay(RoundedRectangle(cornerRadius: 6, style: .continuous).strokeBorder(N.line))
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
            PixelGlyph(activity: live ? .watching : .idle, size: 10)
            Text(text).font(.system(size: 12, weight: .medium)).foregroundStyle(N.text2)
        }.padding(.horizontal, 10).frame(height: 26).liquidGlassCapsule()
    }
}



// MARK: - Looks (selectable variants)

enum NekoAppearance: String, CaseIterable, Identifiable {
    case system, light, dark
    var id: String { rawValue }
    var title: String {
        switch self { case .system: "System"; case .light: "Light"; case .dark: "Dark" }
    }
    var colorScheme: ColorScheme? {
        switch self { case .system: nil; case .light: .light; case .dark: .dark }
    }
}

enum NekoLook: String, CaseIterable, Identifiable {
    case system, ambient, dense, mascot
    var id: String { rawValue }
    var title: String {
        switch self {
        case .system: "Neutral"
        case .ambient: "Ambient"
        case .dense: "Neutral (legacy)"
        case .mascot: "Ambient with mascot"
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

extension View {
    @ViewBuilder func softScrollEdges() -> some View {
        if #available(macOS 26, *) { self.scrollEdgeEffectStyle(.soft, for: .all) } else { self }
    }
}

extension View {
    @ViewBuilder func glassProminentButton() -> some View {
        if #available(macOS 26, *) { self.buttonStyle(.glassProminent) } else { self.buttonStyle(.borderedProminent) }
    }
}


// MARK: - OKLCH

extension Color {
    /// AppKit resolves these in the hosting window's appearance, including sheets.
    static func adaptive(light: UInt32, dark: UInt32) -> Color {
        Color(nsColor: NSColor(name: nil) { appearance in
            let rgb = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua ? dark : light
            return NSColor(srgbRed: Double((rgb >> 16) & 0xFF) / 255,
                           green: Double((rgb >> 8) & 0xFF) / 255,
                           blue: Double(rgb & 0xFF) / 255, alpha: 1)
        })
    }
    /// OKLCH → sRGB (Björn Ottosson). Out-of-gamut channels are clamped.
    static func oklch(_ l: Double, _ c: Double, _ hDegrees: Double, opacity: Double = 1) -> Color {
        let h = hDegrees * .pi / 180
        let a = c * cos(h), b = c * sin(h)
        let l_ = l + 0.3963377774 * a + 0.2158037573 * b
        let m_ = l - 0.1055613458 * a - 0.0638541728 * b
        let s_ = l - 0.0894841775 * a - 1.2914855480 * b
        let L = l_ * l_ * l_, M = m_ * m_ * m_, S = s_ * s_ * s_
        let r = 4.0767416621 * L - 3.3077115913 * M + 0.2309699292 * S
        let g = -1.2684380046 * L + 2.6097574011 * M - 0.3413193965 * S
        let bl = -0.0041960863 * L - 0.7034186147 * M + 1.7076147010 * S
        return Color(.sRGBLinear, red: min(max(r, 0), 1), green: min(max(g, 0), 1), blue: min(max(bl, 0), 1), opacity: opacity)
    }
}


// MARK: - Arc × CleanMyMac

/// Arc keeps colour on the window "space" and leaves content neutral.
/// Deep indigo into violet, with a soft magenta bloom low right.
struct SpaceGradient: View {
    var intensity: Double = 1
    @Environment(\.accessibilityReduceTransparency) private var opaque
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        Group {
            if let library = GemShaders.library, !opaque {
                GeometryReader { geo in
                    TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: reduceMotion)) { context in
                        let t = Float(context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 3600))
                        Rectangle().fill(.black)
                            .colorEffect(library.gemField(.float2(Float(geo.size.width), Float(geo.size.height)), .float(t)))
                    }
                }
            } else {
                MeshFallback()
            }
        }.opacity(intensity).ignoresSafeArea().accessibilityHidden(true).allowsHitTesting(false)
    }
}

private struct MeshFallback: View {
    var body: some View {
        if #available(macOS 15, *) {
            MeshGradient(width: 3, height: 3,
                points: [.init(0, 0), .init(0.5, 0), .init(1, 0), .init(0, 0.5), .init(0.55, 0.45), .init(1, 0.5), .init(0, 1), .init(0.5, 1), .init(1, 1)],
                colors: [
                    .oklch(0.20, 0.07, 265), .oklch(0.16, 0.05, 275), .oklch(0.19, 0.08, 295),
                    .oklch(0.17, 0.06, 255), .oklch(0.22, 0.09, 285), .oklch(0.24, 0.10, 305),
                    .oklch(0.13, 0.04, 262), .oklch(0.19, 0.08, 290), .oklch(0.22, 0.09, 175)
                ])
        } else {
            LinearGradient(colors: [.oklch(0.18, 0.07, 265), .oklch(0.22, 0.09, 300)], startPoint: .topLeading, endPoint: .bottomTrailing)
        }
    }
}

/// Loads the Metal gem shaders bundled with the app. Nil in tests or if the
/// library is missing, so every caller falls back to plain gradients.
enum GemShaders {
    static let library: ShaderLibrary? = Bundle.main.url(forResource: "Neko", withExtension: "metallib").map { ShaderLibrary(url: $0) }
}

/// Jewel tints: one stone per meaning.
enum Gem {
    static let ruby = Color.oklch(0.58, 0.19, 18)
    static let amethyst = Color.oklch(0.55, 0.17, 300)
    static let sapphire = Color.oklch(0.55, 0.16, 262)
    static let emerald = Color.oklch(0.62, 0.14, 160)
    static let topaz = Color.oklch(0.74, 0.14, 70)
}

/// A shape filled with faceted gem shading, animated by a slow key light.
struct GemSurface<S: Shape>: View {
    let shape: S
    let tint: Color
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        if let library = GemShaders.library {
            GeometryReader { geo in
                TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: reduceMotion)) { context in
                    let t = Float(context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 3600))
                    shape.fill(.white)
                        .colorEffect(library.gemSheen(.float2(Float(geo.size.width), Float(geo.size.height)), .float(t), .color(tint)))
                }
            }
        } else {
            shape.fill(LinearGradient(colors: [tint, tint.opacity(0.6)], startPoint: .topLeading, endPoint: .bottomTrailing))
        }
    }
}

/// CleanMyMac-style dimensional glyph: a glossy tinted tile with an SF Symbol,
/// top highlight, inner rim and a soft coloured drop shadow.
struct DimensionalGlyph: View {
    let symbol: String
    let tint: Color
    var size: CGFloat = 76
    var body: some View {
        ZStack {
            GemSurface(shape: RoundedRectangle(cornerRadius: size * 0.3, style: .continuous), tint: tint)
            RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
                .strokeBorder(LinearGradient(colors: [.white.opacity(0.6), .white.opacity(0.08)], startPoint: .top, endPoint: .bottom), lineWidth: 1)
            Image(systemName: symbol)
                .font(.system(size: size * 0.40, weight: .semibold))
                .foregroundStyle(.white)
                .shadow(color: .black.opacity(0.35), radius: 2, y: 1)
        }
        .frame(width: size, height: size)
        .shadow(color: tint.opacity(0.5), radius: 16, y: 8)
        .accessibilityHidden(true)
    }
}

/// Big-number summary card with a glyph peeking from the top right and a pill action.
struct SummaryCard: View {
    let title: String
    let value: String
    let caption: String
    let symbol: String
    let tint: Color
    var action: String? = nil
    var perform: () -> Void = {}
    @State private var hover = false
    var body: some View {
        ZStack(alignment: .topTrailing) {
            VStack(alignment: .leading, spacing: 0) {
                Text(title).font(.system(size: 13, weight: .medium)).foregroundStyle(.white.opacity(0.78))
                Spacer(minLength: 28)
                Text(value).font(.system(size: 34, weight: .bold)).tracking(-0.6).foregroundStyle(.white).monospacedDigit()
                // Fixed-height row so the big number sits on the same line in every card,
                // whether or not the card has an action.
                HStack(alignment: .center) {
                    Text(caption).font(.system(size: 13)).foregroundStyle(.white.opacity(0.7)).lineLimit(1)
                    Spacer(minLength: 8)
                    if let action {
                        Button(action, action: perform)
                            .buttonStyle(PillButtonStyle())
                    }
                }.frame(height: 28).padding(.top, 2)
            }
            .padding(18)
            .frame(maxWidth: .infinity, minHeight: 168, alignment: .topLeading)
            DimensionalGlyph(symbol: symbol, tint: tint, size: 64).padding(.top, -6).padding(.trailing, 14)
        }
        .background {
            ZStack {
                RoundedRectangle(cornerRadius: 18, style: .continuous).fill(.ultraThinMaterial).opacity(0.55)
                RoundedRectangle(cornerRadius: 18, style: .continuous).fill(Color.black.opacity(0.25))
                RadialGradient(colors: [tint.opacity(hover ? 0.42 : 0.30), .clear], center: .topTrailing, startRadius: 0, endRadius: 240)
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .strokeBorder(AngularGradient(colors: [.white.opacity(0.35), tint.opacity(0.5), .white.opacity(0.08), Gem.sapphire.opacity(0.4), .white.opacity(0.35)], center: .center), lineWidth: 1)
        )
        .scaleEffect(hover ? 1.01 : 1)
        .animation(.spring(response: 0.3, dampingFraction: 0.8), value: hover)
        .onHover { hover = $0 }
        .accessibilityElement(children: .combine)
    }
}

struct PillButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.font(.system(size: 12.5, weight: .semibold)).foregroundStyle(.white)
            .padding(.horizontal, 14).frame(height: 28)
            .background(.white.opacity(configuration.isPressed ? 0.28 : 0.18), in: Capsule())
            .overlay(Capsule().strokeBorder(.white.opacity(0.25)))
            .scaleEffect(configuration.isPressed ? 0.96 : 1)
    }
}

/// Primary action: a polished-stone pill with a slow light sweep.
struct OrbButton: View {
    let title: String
    let perform: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: perform) {
            HStack(spacing: 8) {
                Text(title).font(.system(size: 15, weight: .semibold))
                Image(systemName: "arrow.right").font(.system(size: 13, weight: .semibold))
                    .offset(x: hover ? 2 : 0)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 26).frame(height: 44)
            .background { GemSurface(shape: RoundedRectangle(cornerRadius: 14, style: .continuous), tint: Gem.amethyst) }
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(LinearGradient(colors: [.white.opacity(0.45), .white.opacity(0.06)], startPoint: .top, endPoint: .bottom), lineWidth: 1))
            .shadow(color: Gem.amethyst.opacity(hover ? 0.55 : 0.35), radius: hover ? 22 : 14, y: 6)
            .animation(.spring(response: 0.3, dampingFraction: 0.8), value: hover)
            .contentShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

extension View {
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


// MARK: - Plain (in-app) variants: no colour, no shaders. Gem styling is onboarding-only.

struct PlainSummaryCard: View {
    let title: String
    let value: String
    let caption: String
    let activity: NekoActivity
    var live = false
    var action: String? = nil
    var perform: () -> Void = {}
    @State private var hover = false
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(title).font(.system(size: 13, weight: .medium)).foregroundStyle(N.text3)
                Spacer()
                PixelGlyph(activity: live ? activity : .idle, size: 16, animated: live)
            }
            Spacer(minLength: 20)
            Text(value).font(.system(size: 30, weight: .semibold)).tracking(-0.5).foregroundStyle(N.text).monospacedDigit()
            HStack(alignment: .center) {
                Text(caption).font(.system(size: 12.5)).foregroundStyle(N.text3).lineLimit(1)
                Spacer(minLength: 8)
                if let action { Button(action, action: perform).nekoGlassButton().controlSize(.small) }
            }.frame(height: 28).padding(.top, 2)
        }
        .padding(16)
        .frame(maxWidth: .infinity, minHeight: 140, alignment: .topLeading)
        .background(hover ? N.selected : N.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(N.line))
        .animation(.easeOut(duration: 0.12), value: hover)
        .onHover { hover = $0 }
        .accessibilityElement(children: .combine)
    }
}
