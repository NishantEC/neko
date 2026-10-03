import SwiftUI
import NekoKit

@main struct NekoNativeApp: App {
    @StateObject private var model = AppModel()
    @Environment(\.openWindow) private var openWindow
    init() { NativeLifecycle.startDaemon() }
    var body: some Scene {
        Window("Neko", id: "workspace") {
            WorkspaceView(model: model)
                .tint(NekoStyle.accent)
                .task { PreviousAppContext.shared.start(); PresenceController.shared.start(model: model); await model.start(); await PaletteController.shared.configure(model: model) }
                .onReceive(NotificationCenter.default.publisher(for: .nekoOpenWorkspace)) { _ in openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }
                .onReceive(NotificationCenter.default.publisher(for: .nekoOpenPreferences)) { _ in openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }
        }
        .defaultSize(width: 1280, height: 820)
        .windowResizability(.contentSize)
        .commands {
            CommandGroup(after: .newItem) { Button("Quick panel") { PaletteController.shared.toggle(model: model) }.keyboardShortcut("k", modifiers: [.command]) }
            CommandGroup(replacing: .appSettings) { Button("Settings…") { model.requestedPage = "Settings"; openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }.keyboardShortcut(",", modifiers: [.command]) }
            CommandMenu("Work") {
                Button("Stop all work") { Task { await StopAllWork.run(model) } }.keyboardShortcut(.escape, modifiers: [.command, .shift])
                Button("Clear finished tickets") { Task { await model.workbench(.command("ClearFinishedTasks", ["workspace_id": model.selectedWorkspace.map { .string($0) } ?? .null])) } }
            }
        }
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
        Button("Settings…") { model.requestedPage = "Settings"; openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }
        Divider()
        Button("Stop all work (\(StopAllWork.shortcutLabel))") { Task { await StopAllWork.run(model) } }
        Divider()
        if Uninstaller.available { Button("Uninstall Neko…") { Uninstaller.confirmAndRun(model) } }
        Button("Quit Neko") { NSApp.terminate(nil) }.keyboardShortcut("q")
    }
}

struct WorkspaceView: View {
    @ObservedObject var model: AppModel
    @State private var page = ProcessInfo.processInfo.environment["NEKO_START_PAGE"] ?? "Home"
    @State private var columns: NavigationSplitViewVisibility = ProcessInfo.processInfo.environment["NEKO_SIDEBAR"] == "hidden" ? .detailOnly : .all
    @AppStorage("neko.look") private var look = ProcessInfo.processInfo.environment["NEKO_LOOK"] ?? NekoLook.ambient.rawValue
    @State private var addingWorkspace = false
    private let pages = [("Home", "house"), ("Tickets", "tray"), ("Responsibilities", "waveform.path"), ("Tools & skills", "shippingbox"), ("Schedules", "calendar"), ("Memory", "text.alignleft"), ("Profiles", "person.2"), ("Settings", "gearshape")]
    var body: some View {
        Group {
            if model.loadingSetup {
                VStack(spacing: 20) { ProgressView("Connecting to Neko…"); if model.error != nil { Button("Retry connection") { Task { await model.start() } } } }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if model.onboarding { OnboardingView(model: model) }
            else {
                NavigationSplitView(columnVisibility: $columns) {
                    NativeSidebar(model: model, page: $page, pages: pages, addingWorkspace: $addingWorkspace, look: $look)
                        .stableSplitPane()
                        .navigationSplitViewColumnWidth(min: 200, ideal: 240, max: 300)
                } detail: {
                    // Work and Today (the default page) draw their own toolbar row.
                    let ownsToolbarRow = !["Workspaces", "Responsibilities", "Tools & skills", "Memory", "Profiles", "Schedules", "Settings", "Activity", "Reply views"].contains(page)
                    let isToday = ownsToolbarRow && page != "Tickets" && model.agentID == nil
                    VStack(spacing: 0) {
                    // Today places the banner under its own toolbar row.
                    if !isToday && model.agentID == nil { FullDiskAccessBanner().padding(.top, 40) }
                    Group {
                        if let agentID = model.agentID {
                            TicketDetail(model: model, id: agentID, close: { model.closeAgent() }, fullPage: true).id(agentID)
                        } else { switch page {
                        case "Tickets": TicketsView(model: model, sidebarHidden: columns == .detailOnly)
                        case "Workspaces": WorkspacesView(model: model)
                        case "Responsibilities": ResponsibilitiesView(model: model)
                        case "Tools & skills": ToolsView(model: model)
                        case "Memory": MemoryView(model: model)
                        case "Profiles": ProfilesView(model: model)
                        case "Schedules": SchedulesView(model: model)
                        case "Settings": PreferencesView(model: model)
                        case "Activity": ActivityGallery()
                        case "Reply views": ReplyGallery()
                        default: TodayView(model: model)
                        } }
                    }
                    }
                    .stableSplitPane()
                    .background { LookBackground(look: look) }
                    .softScrollEdges()
                    // No title strip: the page is named by the sidebar selection, and the
                    // content runs to the top edge with only the window controls above it.
                    .environment(\.sidebarCollapsed, columns == .detailOnly)
                    // Pages other than Work keep clear of the window controls strip.
                    // Every page starts at the window's top edge; each page's own
                    // header spacing is enough to clear the window controls.
                    .safeAreaPadding(.top, 0)
                    .navigationTitle("")
                    .hiddenWindowToolbarBackground()
                    .ignoresSafeArea(.container, edges: .top)

                    .onReceive(NotificationCenter.default.publisher(for: .nekoNavigate)) { note in if let key = note.object as? String { model.agentID = nil; page = key == "Today" ? "Home" : key } }
                }
            }
        }
        .frame(minWidth: model.onboarding ? 760 : 960, maxWidth: model.onboarding ? 760 : .infinity, minHeight: model.onboarding ? 580 : 620, maxHeight: model.onboarding ? 580 : .infinity)
        .onChange(of: model.onboarding) { _, onboarding in
            // Let SwiftUI lift the fixed setup frame first, then grow the window.
            if !onboarding { DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { WindowSizer.expandForWorkspace() } }
        }
        .preferredColorScheme(.dark)
        .environment(\.nekoLook, NekoLook(rawValue: look) ?? .ambient)
        .safeAreaInset(edge: .top) {
            if let notice = model.notice, model.error == nil {
                HStack(spacing: 10) { Image(systemName: "checkmark.circle.fill").foregroundStyle(NekoStyle.mint); Text(notice).font(NekoFont.body); Spacer(); Button { model.notice = nil } label: { Image(systemName: "xmark").font(.system(size: 11, weight: .bold)) }.buttonStyle(.plain).accessibilityLabel("Dismiss") }
                    .nekoToast().transition(.move(edge: .top).combined(with: .opacity))
            }
            if let error = model.error {
                HStack(spacing: 10) { Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(NekoStyle.amber); Text(error).font(NekoFont.body).textSelection(.enabled).lineLimit(2); Spacer(); Button("Retry") { Task { await model.refresh() } }.nekoGlassButton(); Button { model.error = nil } label: { Image(systemName: "xmark").font(.system(size: 11, weight: .bold)) }.buttonStyle(.plain).accessibilityLabel("Dismiss") }
                    .nekoToast().transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
        .onReceive(NotificationCenter.default.publisher(for: .nekoOpenPreferences)) { _ in page = "Settings" }
        .onChange(of: model.requestedPage) { _, requested in if let requested { model.agentID = nil; page = requested == "Today" ? "Home" : requested; model.requestedPage = nil } }
        .onAppear {
            if let requested = model.requestedPage { page = requested; model.requestedPage = nil }
            // Development only: NEKO_NOTICE_DEMO="text" shows a notice to check the toast's look.
            if let demo = ProcessInfo.processInfo.environment["NEKO_NOTICE_DEMO"], !demo.isEmpty { model.notice = demo }
        }
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
                StatusPill(text: WatchingPresentation(connected: model.connected, snapshot: model.snapshot).title, live: WatchingPresentation(connected: model.connected, snapshot: model.snapshot).active)
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
                Image(systemName: icon).font(.system(size: 12.5)).frame(width: 16).foregroundStyle(selected ? N.text : N.text3)
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
    @State private var selection: String?
    let pages: [(String, String)]
    @Binding var addingWorkspace: Bool
    @Binding var look: String
    var body: some View {
        List(selection: $selection) {
            Section {
                row("Home")
                row("Tickets")
            }
            ForEach(AgentSidebarGroup.allCases) { group in
                let tasks = group.tasks(in: model.tasks)
                if !tasks.isEmpty {
                    Section(group.title) {
                        ForEach(tasks.prefix(5), id: \.recordID) { task in
                            HStack(spacing: 8) {
                                Circle().fill(ticketStatusColor(task["status"].string)).frame(width: 6, height: 6)
                                Text(task["title"].string).lineLimit(1)
                            }
                            .tag("agent:" + task.recordID)
                            .help("NEK-" + String(task.recordID.prefix(4)).uppercased() + " · " + friendlyTaskStatus(task["status"].string))
                        }
                        if tasks.count > 5 {
                            Text("Show all \(tasks.count)").foregroundStyle(.secondary).tag("group:" + group.rawValue)
                        }
                    }
                }
            }
            ForEach(PageInfo.groups.filter { $0.title != "Your day" }, id: \.title) { group in
                Section(group.title) {
                    ForEach(group.keys, id: \.self) { key in row(key) }
                }
            }
        }
        .listStyle(.sidebar)
        // Where Neko works sits at the top, like a Codex or Xcode project picker.
        .safeAreaInset(edge: .top, spacing: 0) {
            WorkspaceMenu(model: model, addingWorkspace: $addingWorkspace)
                .menuStyle(.button).buttonStyle(.borderless)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 14).padding(.vertical, 8)
        }
        // Status and app controls live in the sidebar footer.
        .safeAreaInset(edge: .bottom, spacing: 0) {
            HStack(spacing: 10) {
                WatchingStatus(connected: model.connected, snapshot: model.snapshot)
                Spacer()
                Menu {
                    Picker("Look", selection: $look) { ForEach(NekoLook.allCases) { Text($0.title).tag($0.rawValue) } }.pickerStyle(.inline)
                } label: { Image(systemName: "paintpalette") }
                .menuStyle(.button).buttonStyle(.borderless).menuIndicator(.hidden).fixedSize()
                .help("Switch Neko's look").accessibilityLabel("Look")
                Button { PaletteController.shared.toggle(model: model) } label: { Image(systemName: "command") }
                    .buttonStyle(.borderless).help("Quick panel (⌘K)").accessibilityLabel("Quick panel")
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, 14).padding(.vertical, 10)
            .overlay(alignment: .top) { N.line.frame(height: 1) }
        }
        .onAppear { selection = page }
        .onChange(of: selection) { _, value in
            guard let value else { return }
            if value.hasPrefix("agent:") { model.openAgent(String(value.dropFirst(6)), from: page) }
            else if value.hasPrefix("group:") { model.showAgents(String(value.dropFirst(6))); page = "Tickets" }
            else { guard value != page || model.agentID != nil else { return }; model.agentID = nil; if value == "Tickets" { model.agentFilter = "all" }; page = value }
        }
        .onChange(of: model.agentID) { _, id in selection = id.map { "agent:" + $0 } ?? page }
        .onChange(of: page) { _, value in if model.agentID == nil { selection = value } }
        .onChange(of: model.selectedWorkspace) { _, _ in model.agentID = nil }
    }
    private func row(_ key: String) -> some View {
        let attention = key == "Tickets" ? AgentSidebarGroup.needsYou.tasks(in: model.tasks).count : 0
        return Label(PageInfo.title(key), systemImage: PageInfo.icon(key)).badge(attention).tag(key)
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
    let snapshot: JSONValue
    var body: some View {
        let status = WatchingPresentation(connected: connected, snapshot: snapshot)
        HStack(spacing: 6) {
            PixelGlyph(activity: status.active ? .watching : .idle, size: 11)
            Text(status.title).font(.callout)
        }.padding(.horizontal, 8).accessibilityElement(children: .combine)
    }
}

struct WatchingPresentation {
    let connected: Bool
    let snapshot: JSONValue
    var count: Int { snapshot["mcp"]["responsibilities"].array.filter { $0["enabled"].bool }.count }
    var active: Bool { connected && count > 0 }
    var title: String {
        if !connected { return "Reconnecting…" }
        return count == 0 ? "Ready" : "Watching \(count)"
    }
}

/// User-facing names follow the product loop: Neko watches, plans, then asks you.
enum PageInfo {
    static let groups: [(title: String, keys: [String])] = [
        ("Your day", ["Home", "Tickets"]),
        ("What Neko watches", ["Workspaces", "Responsibilities", "Schedules"]),
        ("Teach Neko", ["Tools & skills", "Memory", "Profiles"]),
        ("Neko", ["Settings"])
    ]
    static func title(_ key: String) -> String {
        switch key {
        case "Tickets": "All agents"
        case "Responsibilities": "Watching"
        case "Tools & skills": "Tools & skills"
        default: key
        }
    }
    static func icon(_ key: String) -> String {
        switch key {
        case "Home", "Today": "house"
        case "Workspaces": "square.stack.3d.up"
        case "Tickets": "checklist"
        case "Responsibilities": "eye"
        case "Schedules": "clock"
        case "Tools & skills": "wrench.and.screwdriver"
        case "Memory": "brain"
        case "Profiles": "person.2"
        case "Settings": "gearshape"
        default: "circle"
        }
    }
}

/// One place to choose where Neko works, as in Codex and Xcode. Used in the toolbar and composer.
struct WorkspaceMenu: View {
    @ObservedObject var model: AppModel
    @Binding var addingWorkspace: Bool
    var compact = false
    private var current: String { model.selectedWorkspace.flatMap { id in model.workspaces.first { $0.recordID == id }?["name"].string } ?? "All workspaces" }
    var body: some View {
        Menu {
            Button { model.selectedWorkspace = nil } label: { Label("All workspaces", systemImage: model.selectedWorkspace == nil ? "checkmark" : "square.stack") }
            if !model.workspaces.isEmpty { Divider() }
            ForEach(model.workspaces, id: \.recordID) { workspace in
                Button { model.selectedWorkspace = workspace.recordID } label: {
                    Label(workspace["name"].string, systemImage: model.selectedWorkspace == workspace.recordID ? "checkmark" : "folder")
                }
            }
            Divider()
            Button("Add workspace…") { addingWorkspace = true }
            if model.selectedWorkspace != nil { Button("Workspace settings…") { addingWorkspace = true } }
        } label: {
            Label(current, systemImage: "folder").labelStyle(.titleAndIcon)
        }
        .menuIndicator(.visible)
        .help("Choose where Neko works")
        .accessibilityLabel("Workspace: \(current)")
    }
}

/// Arc: content floats in a calm, neutral card; colour stays on the window space around it.
struct ArcCard: ViewModifier {
    let enabled: Bool
    func body(content: Content) -> some View {
        if enabled {
            content
                .background(N.panel.opacity(0.94), in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(.white.opacity(0.08)))
                .shadow(color: .black.opacity(0.35), radius: 16, y: 6)
                .padding([.horizontal, .bottom], 10).padding(.top, 2)
        } else {
            content
        }
    }
}

/// Setup is a small, fixed window; the workspace opens at a comfortable size once setup ends.
@MainActor enum WindowSizer {
    static func expandForWorkspace() {
        guard let window = NSApp.windows.first(where: { $0.isVisible && $0.frame.width >= 600 && $0.styleMask.contains(.titled) }),
              let screen = (window.screen ?? NSScreen.main)?.visibleFrame else { return }
        let size = NSSize(width: min(1280, screen.width - 80), height: min(820, screen.height - 60))
        let origin = NSPoint(x: screen.midX - size.width / 2, y: screen.midY - size.height / 2)
        window.setFrame(NSRect(origin: origin, size: size), display: true, animate: true)
    }
}

/// When the sidebar is hidden, the traffic lights and sidebar toggle float over
/// the top-left of the detail pane; pages leave room for them.
private struct SidebarCollapsedKey: EnvironmentKey { static let defaultValue = false }
extension EnvironmentValues {
    var sidebarCollapsed: Bool {
        get { self[SidebarCollapsedKey.self] }
        set { self[SidebarCollapsedKey.self] = newValue }
    }
}
