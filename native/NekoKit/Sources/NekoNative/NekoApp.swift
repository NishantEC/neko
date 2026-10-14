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
    @State private var page: Page
    init(model: AppModel, page: Page = Page.start()) {
        self.model = model
        _page = State(initialValue: page)
    }
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
                                .navigationTitle(TaskText(task: ticket, snapshot: model.snapshot).title)
                                .navigationSubtitle(([friendlyTaskStatus(ticket["status"].string)] + [TaskText(task: ticket, snapshot: model.snapshot).detail]).filter { !$0.isEmpty }.joined(separator: " · "))
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
        case .workspaces, .watching, .tools, .memory, .profiles, .schedules, .settings:
            // Setup pages are sections of Settings; old links land on the right one.
            SettingsHub(model: model, section: SettingsSection(page: page) ?? .general).id(page)
        case .activity: ActivityGallery()
        case .replyViews: ReplyGallery()
        case .designLab: DesignLabView()
        }
    }
}

/// The sidebar is your work: tasks grouped by what they need, newest first.
/// Setup lives in Settings (the gear in the footer, or ⌘,).
struct NativeSidebar: View {
    @ObservedObject var model: AppModel
    @Binding var page: Page
    /// Row tags: a page's raw value, "agent:<id>" or "stage:<stage>".
    @State private var selection: String?
    @State private var filter = ""
    @Binding var addingWorkspace: Bool
    @AppStorage("nekoAppearance") private var appearance = NekoAppearance.system.rawValue
    private let shown = 5
    private static let stages: [TaskStage] = [.needsYou, .review, .running, .upNext]
    var body: some View {
        let counts = TaskCounts(tasks: model.tasks)
        // Header, list and footer are stacked rather than inset over the list:
        // a sidebar List ignores safe-area insets, which drew rows underneath
        // the header and let them take its clicks.
        VStack(spacing: 0) {
            VStack(alignment: .leading, spacing: NekoSpace.sm) {
                WorkspaceMenu(model: model, addingWorkspace: $addingWorkspace)
                    .menuStyle(.button).nekoButton(.quiet)
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button { newTask() } label: {
                    Label("New task", systemImage: "square.and.pencil").frame(maxWidth: .infinity, alignment: .leading)
                }
                .nekoButton(.secondary).controlSize(.large)
                .keyboardShortcut("n", modifiers: .command)
                .help("Describe what you want done (⌘N)")
                NekoSearchField(title: "Find a task", text: $filter, size: .small)
            }
            .padding(.horizontal, NekoSpace.lg).padding(.vertical, NekoSpace.md)
            List(selection: $selection) {
                Section {
                    row(.home, title: "Home", symbol: "house", badge: 0)
                    row(.tickets, title: "All tasks", symbol: "checklist", badge: counts.waitingOnYou)
                }
                ForEach(Self.stages) { stage in stageSection(stage) }
            }
            .listStyle(.sidebar)
            .font(NekoFont.of(.callout))
            .environment(\.defaultMinListRowHeight, 30)
            Divider()
            HStack(spacing: NekoSpace.md) {
                WatchingStatus(connected: model.connected, snapshot: model.snapshot)
                Spacer()
                Menu {
                    Picker("Appearance", selection: $appearance) { ForEach(NekoAppearance.allCases) { Text($0.title).tag($0.rawValue) } }.pickerStyle(.inline)
                } label: { Image(systemName: "paintpalette") }
                .menuStyle(.button).nekoButton(.quiet).menuIndicator(.hidden).fixedSize()
                .help("Light, dark or match your Mac").accessibilityLabel("Appearance")
                Button { model.agentID = nil; page = .settings } label: { Image(systemName: "gearshape") }
                    .nekoButton(.quiet).help("Settings (⌘,)").accessibilityLabel("Settings")
            }
            .foregroundStyle(.secondary)
            .padding(.horizontal, NekoSpace.lg).padding(.vertical, NekoSpace.md)
        }
        .onAppear { selection = page.rawValue }
        .onChange(of: selection) { _, value in
            guard let value else { return }
            if value.hasPrefix("agent:") { model.openAgent(String(value.dropFirst(6)), from: page) }
            else if value.hasPrefix("stage:") { model.showAgents(String(value.dropFirst(6))); page = .tickets }
            else if let chosen = Page(rawValue: value) { guard chosen != page || model.agentID != nil else { return }; model.agentID = nil; if chosen == .tickets { model.agentFilter = "all" }; page = chosen }
        }
        .onChange(of: model.agentID) { _, id in selection = id.map { "agent:" + $0 } ?? page.rawValue }
        .onChange(of: page) { _, value in if model.agentID == nil { selection = value.rawValue } }
        .onChange(of: model.selectedWorkspace) { _, _ in model.agentID = nil }
    }
    /// New task: go to Home and put the cursor in its composer.
    private func newTask() {
        model.agentID = nil
        page = .home
        ComposerFocus.request()
    }
    private func matches(_ task: JSONValue) -> Bool {
        let query = filter.trimmingCharacters(in: .whitespaces)
        if query.isEmpty { return true }
        let text = TaskText(task: task, snapshot: model.snapshot)
        return text.title.localizedCaseInsensitiveContains(query) || text.detail.localizedCaseInsensitiveContains(query) || task["title"].string.localizedCaseInsensitiveContains(query)
    }
    @ViewBuilder private func stageSection(_ stage: TaskStage) -> some View {
        let tasks = model.tasks.filter { TaskStage(status: $0["status"].string) == stage && matches($0) }
            .sorted { stage == .upNext ? $0["created_at_ms"].int < $1["created_at_ms"].int : $0["updated_at_ms"].int > $1["updated_at_ms"].int }
        // The queue only needs to say what starts next; the rest is a click away.
        let limit = stage == .upNext && filter.isEmpty ? 1 : shown
        if !tasks.isEmpty {
            Section {
                ForEach(tasks.prefix(limit), id: \.recordID) { task in taskRow(task, stage: stage) }
                if tasks.count > limit {
                    Text("Show all \(tasks.count)").foregroundStyle(.secondary).tag("stage:" + stage.rawValue)
                }
            } header: {
                HStack {
                    Text(stage.title)
                    Spacer()
                    Text(tasks.count, format: .number).monospacedDigit()
                }.accessibilityElement(children: .combine)
            }
        }
    }
    private func taskRow(_ task: JSONValue, stage: TaskStage) -> some View {
        let text = TaskText(task: task, snapshot: model.snapshot)
        return HStack(spacing: NekoSpace.sm) {
            Circle().fill(stage.color).frame(width: 7, height: 7).frame(width: 16).accessibilityHidden(true)
            Text(text.title).lineLimit(1)
        }
        .tag("agent:" + task.recordID)
        .help([text.title, text.detail, text.sourceTitle].compactMap { $0?.isEmpty == false ? $0 : nil }.joined(separator: "\n"))
        .accessibilityLabel("\(text.title), \(stage.title)")
    }
    private func row(_ item: Page, title: String, symbol: String, badge: Int) -> some View {
        Button {
            model.agentID = nil
            if item == .tickets { model.showAgents() }
            page = item
        } label: {
            Label(title, systemImage: symbol).frame(maxWidth: .infinity, alignment: .leading).contentShape(Rectangle())
        }.buttonStyle(.plain).badge(badge).tag(item.rawValue)
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
