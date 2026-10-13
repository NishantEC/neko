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
            CommandGroup(replacing: .appSettings) { Button("Settings…") { model.requestedPage = .settings; openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }.keyboardShortcut(",", modifiers: [.command]) }
            CommandMenu("Work") {
                Button("Stop all work") { Task { await StopAllWork.run(model) } }.keyboardShortcut(.escape, modifiers: [.command, .shift])
                Button("Clear finished tickets") { Task { await model.workbench(.command("ClearFinishedTasks", ["workspace_id": model.selectedWorkspace.map { .string($0) } ?? .null])) } }
            }
        }
        MenuBarExtra { StatusMenu(model: model) } label: {
            // The icon says whether anything needs you, without opening the menu.
            let state = MenuBarState(StatusSummary(snapshot: model.snapshot))
            Image(nsImage: MenuBarIcon.image(for: state)).accessibilityLabel(state.accessibilityLabel)
        }
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
        Button("Settings…") { model.requestedPage = .settings; openWindow(id: "workspace"); NSApp.activate(ignoringOtherApps: true) }
        Divider()
        Button("Stop all work (\(StopAllWork.shortcutLabel))") { Task { await StopAllWork.run(model) } }
        Divider()
        if Uninstaller.available { Button("Uninstall Neko…") { Uninstaller.confirmAndRun(model) } }
        Button("Quit Neko") { NSApp.terminate(nil) }.keyboardShortcut("q")
    }
}

struct WorkspaceView: View {
    @ObservedObject var model: AppModel
    @State private var page = Page.start()
    @State private var columns: NavigationSplitViewVisibility = ProcessInfo.processInfo.environment["NEKO_SIDEBAR"] == "hidden" ? .detailOnly : .all
    @AppStorage("neko.look") private var look = ProcessInfo.processInfo.environment["NEKO_LOOK"] ?? NekoLook.ambient.rawValue
    @AppStorage("nekoAppearance") private var appearance = NekoAppearance.system.rawValue
    @State private var addingWorkspace = false
    var body: some View {
        Group {
            if model.loadingSetup {
                VStack(spacing: 20) { ProgressView("Connecting to Neko…"); if model.error != nil { Button("Retry connection") { Task { await model.start() } } } }.frame(maxWidth: .infinity, maxHeight: .infinity)
            } else if model.onboarding { OnboardingView(model: model) }
            else {
                NavigationSplitView(columnVisibility: $columns) {
                    NativeSidebar(model: model, page: $page, addingWorkspace: $addingWorkspace)
                        .stableSplitPane()
                        .navigationSplitViewColumnWidth(min: 220, ideal: 252, max: 320)
                } detail: {
                    // Home and agent chats place their permission banners within their content.
                    let isToday = page == .home && model.agentID == nil
                    VStack(spacing: 0) {
                    // Other pages share the permission banner here.
                    if !isToday && model.agentID == nil { FullDiskAccessBanner() }
                    Group {
                        if let agentID = model.agentID {
                            let ticket = model.tasks.first { $0.recordID == agentID } ?? .null
                            TicketDetail(model: model, id: agentID, close: { model.closeAgent() }, fullPage: true)
                                .navigationTitle("NEK-" + String(agentID.prefix(4)).uppercased() + "  " + ticket["title"].string)
                                .navigationSubtitle(friendlyTaskStatus(ticket["status"].string))
                                .id(agentID)
                        } else { content(for: page) }
                    }
                    }
                    .stableSplitPane()
                    .clipped()
                    .background(N.canvas)
                    .softScrollEdges()
                    // Pages supply real native toolbar items, so the top row is both
                    // useful and clickable. Only backgrounds extend behind the toolbar.
                    .hiddenWindowToolbarBackground()

                }
            }
        }
        .frame(minWidth: model.onboarding ? 840 : 960, maxWidth: model.onboarding ? 840 : .infinity, minHeight: model.onboarding ? 660 : 620, maxHeight: model.onboarding ? 660 : .infinity)
        .onChange(of: model.onboarding) { _, onboarding in
            // Let SwiftUI lift the fixed setup frame first, then grow the window.
            if !onboarding { DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { WindowSizer.expandForWorkspace() } }
        }
        .preferredColorScheme((NekoAppearance(rawValue: appearance) ?? .system).colorScheme)
        .environment(\.nekoLook, NekoLook(rawValue: look) ?? .ambient)
        .safeAreaInset(edge: .top) {
            if let notice = model.notice, model.error == nil {
                NekoNotice(kind: .success, text: notice) { model.notice = nil }.transition(.move(edge: .top))
            }
            if let error = model.error {
                NekoNotice(kind: .warning, text: error) { model.error = nil } actions: {
                    Button("Retry") { Task { await model.refresh() } }.nekoButton(.secondary)
                }.transition(.move(edge: .top))
            }
        }
        .sheet(isPresented: $addingWorkspace) { WorkspaceEditor(model: model) }
        .onReceive(NotificationCenter.default.publisher(for: .nekoOpenPreferences)) { _ in model.requestedPage = .settings }
        .onChange(of: model.requestedPage) { _, requested in if let requested { model.agentID = nil; page = requested; model.requestedPage = nil } }
        .onAppear {
            if let requested = model.requestedPage { page = requested; model.requestedPage = nil }
            // Development only: NEKO_NOTICE_DEMO="text" shows a notice to check the toast's look.
            if let demo = ProcessInfo.processInfo.environment["NEKO_NOTICE_DEMO"], !demo.isEmpty { model.notice = demo }
        }
    }
    /// The screen for each page. One switch, typed, so a new page cannot be forgotten.
    @ViewBuilder private func content(for page: Page) -> some View {
        switch page {
        case .home: TodayView(model: model)
        case .tickets: TicketsView(model: model)
        case .workspaces: WorkspacesView(model: model)
        case .watching: ResponsibilitiesView(model: model)
        case .tools: ToolsView(model: model)
        case .memory: MemoryView(model: model)
        case .profiles: ProfilesView(model: model)
        case .schedules: SchedulesView(model: model)
        case .settings: PreferencesView(model: model)
        case .activity: ActivityGallery()
        case .replyViews: ReplyGallery()
        case .designLab: DesignLabView()
        }
    }
}

/// Native source-list sidebar: on macOS 26 this is the system's floating Liquid Glass sidebar.
struct NativeSidebar: View {
    @ObservedObject var model: AppModel
    @Binding var page: Page
    /// Row tags: a page's raw value, "agent:<id>" or "group:<filter>".
    @State private var selection: String?
    @Binding var addingWorkspace: Bool
    @AppStorage("nekoAppearance") private var appearance = NekoAppearance.system.rawValue
    var body: some View {
        List(selection: $selection) {
            Section {
                row(.home)
                row(.tickets)
            }
            ForEach(AgentSidebarGroup.allCases) { group in agentSection(group) }
            ForEach(Page.sidebarGroups, id: \.title) { group in
                Section(group.title) {
                    ForEach(group.pages) { item in row(item) }
                }
            }
        }
        .listStyle(.sidebar)
        .font(NekoFont.of(.callout))
        .environment(\.defaultMinListRowHeight, 32)
        // Where Neko works sits at the top, like a Codex or Xcode project picker.
        .safeAreaInset(edge: .top, spacing: 0) {
            WorkspaceMenu(model: model, addingWorkspace: $addingWorkspace)
                .menuStyle(.button).nekoButton(.quiet)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 16).padding(.vertical, 12)
        }
        // Status and app controls live in the sidebar footer.
        .safeAreaInset(edge: .bottom, spacing: 0) {
            HStack(spacing: 10) {
                WatchingStatus(connected: model.connected, snapshot: model.snapshot)
                Spacer()
                Menu {
                    Picker("Appearance", selection: $appearance) { ForEach(NekoAppearance.allCases) { Text($0.title).tag($0.rawValue) } }.pickerStyle(.inline)
                } label: { Image(systemName: "paintpalette") }
                .menuStyle(.button).nekoButton(.quiet).menuIndicator(.hidden).fixedSize()
                .help("Choose light, dark, or system appearance").accessibilityLabel("Appearance")
                Button { PaletteController.shared.toggle(model: model) } label: { Image(systemName: "command") }
                    .nekoButton(.quiet).help("Quick panel (⌘K)").accessibilityLabel("Quick panel")
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, 16).padding(.vertical, 14)
        }
        .onAppear { selection = page.rawValue }
        .onChange(of: selection) { _, value in
            guard let value else { return }
            if value.hasPrefix("agent:") { model.openAgent(String(value.dropFirst(6)), from: page) }
            else if value.hasPrefix("group:") { model.showAgents(String(value.dropFirst(6))); page = .tickets }
            else if let chosen = Page(rawValue: value) { guard chosen != page || model.agentID != nil else { return }; model.agentID = nil; if chosen == .tickets { model.agentFilter = "all" }; page = chosen }
        }
        .onChange(of: model.agentID) { _, id in selection = id.map { "agent:" + $0 } ?? page.rawValue }
        .onChange(of: page) { _, value in if model.agentID == nil { selection = value.rawValue } }
        .onChange(of: model.selectedWorkspace) { _, _ in model.agentID = nil }
    }
    @ViewBuilder private func agentSection(_ group: AgentSidebarGroup) -> some View {
        let tasks = group.tasks(in: model.tasks)
        if !tasks.isEmpty {
            Section {
                ForEach(tasks.prefix(5), id: \.recordID) { task in
                    agentRow(task)
                }
                if tasks.count > 5 {
                    Text("Show all \(tasks.count)").foregroundStyle(.secondary).tag("group:" + group.rawValue)
                }
            } header: {
                HStack {
                    Text(group.title)
                    Spacer()
                    Text(tasks.count, format: .number).monospacedDigit()
                }.accessibilityElement(children: .combine)
            }
        }
    }
    private func agentRow(_ task: JSONValue) -> some View {
        let title = task["title"].string
        let status = task["status"].string
        let ticketCode = String(task.recordID.prefix(4)).uppercased()
        let tooltip = "\(title)\nNEK-\(ticketCode) · \(friendlyTaskStatus(status))"
        return HStack(spacing: 8) {
            Circle().fill(ticketStatusColor(status))
                .frame(width: 6, height: 6).frame(width: 18).accessibilityHidden(true)
            Text(title).lineLimit(1)
        }
        .tag("agent:" + task.recordID)
        .help(tooltip)
    }
    private func row(_ item: Page) -> some View {
        let attention = item == .tickets ? AgentSidebarGroup.needsYou.tasks(in: model.tasks).count : 0
        return Button {
            model.agentID = nil
            if item == .tickets { model.showAgents() }
            page = item
        } label: {
            Label(item.title, systemImage: item.symbol).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
        }.buttonStyle(.plain).badge(attention).tag(item.rawValue)
    }
    private func workspaceRow(_ id: String?, name: String, color: Color) -> some View {
        let selected = model.selectedWorkspace == id
        return Button { model.selectedWorkspace = id } label: {
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: NekoRadius.micro, style: .continuous).fill(color).frame(width: 10, height: 10).frame(width: 18)
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
            Circle().fill(status.active ? NekoStyle.mint : Color.secondary)
                .frame(width: 6, height: 6).accessibilityHidden(true)
            Text(status.title).font(NekoFont.meta)
        }.accessibilityElement(children: .combine)
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
