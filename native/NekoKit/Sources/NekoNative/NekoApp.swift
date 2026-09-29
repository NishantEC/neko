import SwiftUI
import NekoKit

@main struct NekoNativeApp: App {
    @StateObject private var model = AppModel()
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings
    init() { NativeLifecycle.startDaemon() }
    var body: some Scene {
        Window("Neko", id: "workspace") {
            WorkspaceView(model: model)
                .tint(NekoStyle.accent)
                .task { await model.start(); await PaletteController.shared.configure(model: model) }
                .onReceive(NotificationCenter.default.publisher(for: .nekoOpenWorkspace)) { _ in openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }
                .onReceive(NotificationCenter.default.publisher(for: .nekoOpenPreferences)) { _ in openSettings() }
        }
        .defaultSize(width: 1440, height: 900)
        .commands {
            CommandGroup(after: .newItem) { Button("Quick panel") { PaletteController.shared.toggle(model: model) }.keyboardShortcut("k", modifiers: [.command]) }
        }
        Settings { PreferencesView(model: model) }
        MenuBarExtra("Neko", systemImage: "cat") { StatusMenu(model: model) }
    }
}

struct StatusMenu: View {
    @ObservedObject var model: AppModel
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Text(StatusSummary(snapshot: model.snapshot).attentionLabel)
        Text(StatusSummary(snapshot: model.snapshot).workingLabel)
        Divider()
        Button("Quick panel") { PaletteController.shared.toggle(model: model) }
        Button("Open Neko") { openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }
        SettingsLink { Text("Settings…") }
        Divider()
        Button("Quit Neko") { NSApp.terminate(nil) }.keyboardShortcut("q")
    }
}

struct WorkspaceView: View {
    @ObservedObject var model: AppModel
    @State private var page = ProcessInfo.processInfo.environment["NEKO_START_PAGE"] ?? "Today"
    @AppStorage("neko.look") private var look = ProcessInfo.processInfo.environment["NEKO_LOOK"] ?? NekoLook.ambient.rawValue
    @State private var addingWorkspace = false
    private let pages = [("Today", "sun.max"), ("Tickets", "tray"), ("Responsibilities", "waveform.path"), ("Tools & skills", "shippingbox"), ("Schedules", "calendar"), ("Memory", "text.alignleft"), ("Profiles", "person.2")]
    var body: some View {
        Group {
            if model.loadingSetup {
                VStack(spacing: 20) { ProgressView("Connecting to Neko…"); if model.error != nil { Button("Retry connection") { Task { await model.start() } } } }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if model.onboarding { OnboardingView(model: model) }
            else {
                NavigationSplitView {
                    NativeSidebar(model: model, page: $page, pages: pages, addingWorkspace: $addingWorkspace)
                        .navigationSplitViewColumnWidth(min: 200, ideal: 232, max: 300)
                } detail: {
                    Group {
                        switch page {
                        case "Tickets": TicketsView(model: model)
                        case "Responsibilities": ResponsibilitiesView(model: model)
                        case "Tools & skills": ToolsView(model: model)
                        case "Memory": MemoryView(model: model)
                        case "Profiles": ProfilesView(model: model)
                        case "Schedules": SchedulesView(model: model)
                        default: TodayView(model: model)
                        }
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background { LookBackground(look: look) }
                    .softScrollEdges()
                    .navigationTitle(page)
                    .navigationSubtitle(model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces")
                    .toolbar {
                        ToolbarItem(placement: .status) { WatchingStatus(connected: model.connected) }
                        ToolbarItemGroup(placement: .primaryAction) {
                            Menu {
                                Picker("Look", selection: $look) { ForEach(NekoLook.allCases) { Text($0.title).tag($0.rawValue) } }.pickerStyle(.inline)
                            } label: { Label("Look", systemImage: "paintpalette") }
                            .help("Switch Neko's look")
                            Button { model.selectedWorkspace = nil; addingWorkspace = true } label: { Label("Add workspace", systemImage: "folder.badge.plus") }
                            Button { PaletteController.shared.toggle(model: model) } label: { Label("Quick panel", systemImage: "command") }.help("Quick panel (⌘K)")
                        }
                    }
                }
            }
        }
        .frame(minWidth: 960, maxWidth: .infinity, minHeight: 620, maxHeight: .infinity)
        .preferredColorScheme(.dark)
        .environment(\.nekoLook, NekoLook(rawValue: look) ?? .ambient)
        .safeAreaInset(edge: .top) {
            if let error = model.error {
                HStack(spacing: 10) { Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(NekoStyle.amber); Text(error).font(NekoFont.body).textSelection(.enabled).lineLimit(2); Spacer(); Button("Retry") { Task { await model.refresh() } }.nekoGlassButton(); Button { model.error = nil } label: { Image(systemName: "xmark").font(.system(size: 11, weight: .bold)) }.buttonStyle(.plain).accessibilityLabel("Dismiss") }
                    .nekoCard(padding: 12, radius: 12).padding(.horizontal, 16).padding(.top, 8).transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
    }
}

struct NekoSidebar: View {
    @ObservedObject var model: AppModel
    @Binding var page: String
    let pages: [(String, String)]
    @Binding var addingWorkspace: Bool
    @Namespace private var selection
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Color.clear.frame(height: 52) // native traffic lights live here
            HStack(spacing: 8) {
                BrandMark(size: 20).clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))
                Text("Neko").font(.system(size: 13, weight: .semibold)).foregroundStyle(N.text)
                Spacer()
                Button { PaletteController.shared.toggle(model: model) } label: {
                    Image(systemName: "magnifyingglass").font(.system(size: 12)).foregroundStyle(N.text4).frame(width: 24, height: 24).contentShape(Rectangle())
                }.buttonStyle(.plain).help("Quick panel ⌘K").accessibilityLabel("Open quick panel")
            }.padding(.horizontal, 10).frame(height: 32)
            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    GlassGroup(spacing: 2) { VStack(alignment: .leading, spacing: 1) { ForEach(pages, id: \.0) { item in navRow(item.0, icon: item.1) } } }
                    HStack {
                        Text("Workspaces").font(.system(size: 12, weight: .medium)).foregroundStyle(N.text4)
                        Spacer()
                        Button { model.selectedWorkspace = nil; addingWorkspace = true } label: { Image(systemName: "plus").font(.system(size: 11)).foregroundStyle(N.text4).frame(width: 20, height: 20).contentShape(Rectangle()) }.buttonStyle(.plain).accessibilityLabel("Add workspace")
                    }.padding(.horizontal, 10).frame(height: 26).padding(.top, 23)
                    workspaceRow(nil, name: "All workspaces", color: Color.white.opacity(0.22))
                    ForEach(Array(model.workspaces.enumerated()), id: \.element.recordID) { index, workspace in
                        workspaceRow(workspace.recordID, name: workspace["name"].string, color: workspaceColor(index))
                    }
                }.padding(.top, 12)
            }.scrollIndicators(.never)
            HStack(spacing: 8) {
                StatusPill(text: model.connected ? "Watching" : "Reconnecting…", live: model.connected)
                Spacer()
                if model.selectedWorkspace != nil {
                    Button { addingWorkspace = true } label: { Image(systemName: "gearshape").font(.system(size: 11)).foregroundStyle(N.text4) }.buttonStyle(.plain).help("Workspace settings").accessibilityLabel("Workspace settings")
                }
                Text("⌘K").font(.system(size: 11)).foregroundStyle(N.text4)
            }.padding(.horizontal, 10).frame(height: 44)
        }
        .padding(.horizontal, 10)
    }
    private func navRow(_ title: String, icon: String) -> some View {
        let selected = page == title
        let attention = title == "Tickets" ? StatusSummary(snapshot: model.snapshot).needsAttention : 0
        return Button { withAnimation(.spring(response: 0.25, dampingFraction: 0.9)) { page = title } } label: {
            HStack(spacing: 10) {
                Image(systemName: icon).font(.system(size: 12.5)).frame(width: 16).foregroundStyle(selected ? N.text : Color(white: 0.49))
                Text(title).font(.system(size: 13, weight: selected ? .medium : .regular)).foregroundStyle(selected ? N.text : N.text2)
                Spacer()
                if attention > 0 {
                    Text(String(attention)).font(.system(size: 11, weight: .semibold).monospacedDigit()).foregroundStyle(N.canvas)
                        .padding(.horizontal, 6).frame(height: 16).background(NekoStyle.accent, in: Capsule())
                }
            }
            .padding(.horizontal, 10).frame(height: N.rowHeight)
            .modifier(SelectedGlass(selected: selected, namespace: selection))
            .contentShape(Rectangle())
        }.buttonStyle(.plain).accessibilityAddTraits(selected ? .isSelected : [])
    }
    private func workspaceRow(_ id: String?, name: String, color: Color) -> some View {
        let selected = model.selectedWorkspace == id
        return Button { model.selectedWorkspace = id; if id != nil { page = "Today" } } label: {
            HStack(spacing: 10) {
                RoundedRectangle(cornerRadius: 2, style: .continuous).fill(color).frame(width: 8, height: 8).frame(width: 16)
                Text(name).font(.system(size: 13)).foregroundStyle(selected ? N.text : N.text2).lineLimit(1)
                Spacer()
            }.padding(.horizontal, 10).frame(height: N.rowHeight).contentShape(Rectangle())
                .background(selected ? N.selected : .clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        }.buttonStyle(.plain)
    }
}

func workspaceColor(_ index: Int) -> Color { [NekoStyle.mint, NekoStyle.sky, NekoStyle.lilac, NekoStyle.amber, NekoStyle.rose, NekoStyle.coral][index % 6] }

/// Glass sits behind the row's own content, so the label stays sharp;
/// the shared ID lets the pill morph between rows.
struct SelectedGlass: ViewModifier {
    let selected: Bool
    let namespace: Namespace.ID
    func body(content: Content) -> some View {
        if selected { content.liquidGlass(radius: 8, interactive: true).glassMorphID("nav", in: namespace) }
        else { content }
    }
}

/// Native source-list sidebar: on macOS 26 this is the system's floating Liquid Glass sidebar.
struct NativeSidebar: View {
    @ObservedObject var model: AppModel
    @Binding var page: String
    let pages: [(String, String)]
    @Binding var addingWorkspace: Bool
    var body: some View {
        List(selection: Binding<String?>(get: { page }, set: { if let v = $0 { page = v } })) {
            Section {
                ForEach(pages, id: \.0) { item in
                    let attention = item.0 == "Tickets" ? StatusSummary(snapshot: model.snapshot).needsAttention : 0
                    Label(item.0, systemImage: item.1).tag(item.0).badge(attention)
                }
            }
            Section("Workspaces") {
                workspaceRow(nil, name: "All workspaces", color: .secondary)
                ForEach(Array(model.workspaces.enumerated()), id: \.element.recordID) { index, workspace in
                    workspaceRow(workspace.recordID, name: workspace["name"].string, color: workspaceColor(index))
                }
                Button { model.selectedWorkspace = nil; addingWorkspace = true } label: { Label("Add workspace", systemImage: "plus") }
                    .buttonStyle(.plain).foregroundStyle(.secondary)
            }
        }
        .listStyle(.sidebar)
    }
    private func workspaceRow(_ id: String?, name: String, color: Color) -> some View {
        let selected = model.selectedWorkspace == id
        return Button { model.selectedWorkspace = id } label: {
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 3, style: .continuous).fill(color).frame(width: 10, height: 10).frame(width: 18)
                Text(name).lineLimit(1)
                Spacer()
                if selected { Image(systemName: "checkmark").font(.caption.weight(.semibold)).foregroundStyle(.secondary) }
            }.contentShape(Rectangle())
        }.buttonStyle(.plain).accessibilityAddTraits(selected ? .isSelected : [])
    }
}

struct WatchingStatus: View {
    let connected: Bool
    var body: some View {
        HStack(spacing: 6) {
            Circle().fill(connected ? NekoStyle.accent : Color.secondary).frame(width: 7, height: 7)
            Text(connected ? "Watching" : "Reconnecting…").font(.callout)
        }.padding(.horizontal, 8).accessibilityElement(children: .combine)
    }
}
