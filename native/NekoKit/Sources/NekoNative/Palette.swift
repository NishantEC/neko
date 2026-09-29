import AppKit
import Carbon
import SwiftUI
import NekoKit

enum PaletteKeyboardRouting {
    enum Action: Equatable { case escape, up, down, pageUp, pageDown, first, last, activate, actions }
    static func action(key: UInt16, modifiers: NSEvent.ModifierFlags, composing: Bool, presentationOpen: Bool, actionsOpen: Bool = false) -> Action? {
        guard !composing, !presentationOpen || actionsOpen else { return nil }
        let editing = modifiers.intersection([.command, .control, .option, .shift])
        if key == 40, editing == .command { return .actions }
        guard editing.isEmpty else { return nil }
        switch key {
        case 53: return .escape
        case 125: return .down
        case 126: return .up
        case 116: return .pageUp
        case 121: return .pageDown
        case 115: return .first
        case 119: return .last
        case 36, 76: return .activate
        default: return nil
        }
    }
}

enum NativeHotkeyCodes {
    private static let names: [String: UInt32] = [
            "Space": 49, "Enter": 36, "Tab": 48, "Backquote": 50, "Escape": 53,
            "KeyA": 0, "KeyS": 1, "KeyD": 2, "KeyF": 3, "KeyH": 4, "KeyG": 5, "KeyZ": 6, "KeyX": 7, "KeyC": 8, "KeyV": 9,
            "KeyB": 11, "KeyQ": 12, "KeyW": 13, "KeyE": 14, "KeyR": 15, "KeyY": 16, "KeyT": 17, "KeyO": 31,
            "KeyU": 32, "KeyI": 34, "KeyP": 35, "KeyL": 37, "KeyJ": 38, "KeyK": 40, "KeyN": 45, "KeyM": 46,
            "Digit1": 18, "Digit2": 19, "Digit3": 20, "Digit4": 21, "Digit5": 23, "Digit6": 22, "Digit7": 26, "Digit8": 28, "Digit9": 25, "Digit0": 29,
            "Minus": 27, "Equal": 24, "BracketLeft": 33, "BracketRight": 30, "Backslash": 42, "Semicolon": 41, "Quote": 39, "Comma": 43, "Period": 47, "Slash": 44,
            "Backspace": 51, "Delete": 117, "Home": 115, "End": 119, "PageUp": 116, "PageDown": 121,
            "ArrowLeft": 123, "ArrowRight": 124, "ArrowDown": 125, "ArrowUp": 126,
            "F1": 122, "F2": 120, "F3": 99, "F4": 118, "F5": 96, "F6": 97, "F7": 98, "F8": 100, "F9": 101, "F10": 109,
            "F11": 103, "F12": 111, "F13": 105, "F14": 107, "F15": 113, "F16": 106, "F17": 64, "F18": 79, "F19": 80, "F20": 90,
            "Numpad0": 82, "Numpad1": 83, "Numpad2": 84, "Numpad3": 85, "Numpad4": 86, "Numpad5": 87, "Numpad6": 88, "Numpad7": 89, "Numpad8": 91, "Numpad9": 92,
            "NumpadDecimal": 65, "NumpadMultiply": 67, "NumpadAdd": 69, "NumpadDivide": 75, "NumpadEnter": 76, "NumpadSubtract": 78, "NumpadEqual": 81, "NumLock": 71,
            "IntlBackslash": 10, "IntlYen": 93, "IntlRo": 94, "KanaMode": 104, "Lang2": 102,
            "AudioVolumeUp": 72, "AudioVolumeDown": 73, "AudioVolumeMute": 74
        ]
    static func code(_ key: String) -> UInt32? { names[key] }
    static func name(for code: UInt16) -> String? { names.first { $0.value == UInt32(code) }?.key }
}

enum PaletteSearchScope {
    static func request(query: String, mode: String?) -> JSONValue {
        let commands = mode == nil && query.hasPrefix("/")
        return .command("Search", ["query": .string(commands ? String(query.dropFirst()) : query), "limit": .number(mode == nil ? 20 : 100), "provider": commands ? .string("command") : mode.map(JSONValue.string) ?? .null])
    }
}

enum PalettePresentation {
    static func preview(_ row: JSONValue) -> String {
        if row["preview"] != .null { return row["preview"].string }
        return row["kind"].string == "clipboard" ? row["id"].string : ""
    }
    static func meterFraction(_ meter: JSONValue) -> Double {
        guard case .number(let fraction) = meter["fraction"], fraction.isFinite else { return 0 }
        return min(1, max(0, fraction))
    }
    static func hasDetail(_ row: JSONValue) -> Bool {
        !preview(row).isEmpty || !row["images"].array.isEmpty
    }
}

@MainActor enum PaletteWindowOwnership {
    static func contains(_ window: NSWindow, root: NSWindow?) -> Bool {
        var current: NSWindow? = window
        while let candidate = current {
            if candidate === root { return true }
            current = candidate.parent ?? candidate.sheetParent
        }
        return false
    }
}

private final class SearchPanel: NSPanel {
    override var canBecomeKey: Bool { true }
    override var canBecomeMain: Bool { false }
}

@MainActor final class PaletteController: NSObject, NSWindowDelegate {
    static let shared = PaletteController()
    private var panel: SearchPanel?
    private var content: PaletteState?
    private weak var model: AppModel?
    private var previousApplication: NSRunningApplication?
    private var hotkey: EventHotKeyRef?
    private var handler: EventHandlerRef?
    private var registeredKey: UInt32?
    private var registeredModifiers: UInt32?
    private var outsideMonitor: Any?
    private var localMonitor: Any?
    private var resignObserver: NSObjectProtocol?

    func configure(model: AppModel) async {
        self.model = model
        NativeHotkeySettings.register = { [weak self] combo in
            guard let self else { throw DaemonClientError.disconnected }
            try self.register(combo: combo)
        }
        if handler == nil {
            var event = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
            InstallEventHandler(GetApplicationEventTarget(), { _, _, _ in
                Task { @MainActor in
                    let controller = PaletteController.shared
                    if let model = controller.model { controller.toggle(model: model) }
                }
                return noErr
            }, 1, &event, nil, &handler)
        }
        var combo: JSONValue = .object(["key": .string("Space"), "modifiers": .array([.string("Alt")])])
        do {
            combo = try await model.request(.string("GetHotkey"))["Hotkey"]["config"]["combo"]
        } catch { model.error = error.localizedDescription }
        do { try register(combo: combo) } catch { model.error = error.localizedDescription }
    }

    private func register(combo: JSONValue) throws {
        guard let key = NativeHotkeyCodes.code(combo["key"].string) else {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "This shortcut key is not supported by the native client."])
        }
        let modifiers = combo["modifiers"].array.reduce(UInt32(0)) { result, modifier in
            result | (["Cmd": UInt32(cmdKey), "Alt": UInt32(optionKey), "Ctrl": UInt32(controlKey), "Shift": UInt32(shiftKey)][modifier.string] ?? 0)
        }
        if registeredKey == key, registeredModifiers == modifiers { return }
        var replacement: EventHotKeyRef?
        let status = RegisterEventHotKey(key, modifiers, EventHotKeyID(signature: 0x4E454B4F, id: 1), GetApplicationEventTarget(), 0, &replacement)
        if status == noErr {
            if let hotkey { UnregisterEventHotKey(hotkey) }
            hotkey = replacement
            registeredKey = key
            registeredModifiers = modifiers
        } else { throw NSError(domain: "Neko", code: Int(status), userInfo: [NSLocalizedDescriptionKey: "The Neko shortcut is already in use. The previous shortcut is still active."]) }
    }

    func toggle(model: AppModel) {
        if panel?.isVisible == true { dismiss(); return }
        self.model = model
        let frontmost = NSWorkspace.shared.frontmostApplication
        previousApplication = frontmost?.processIdentifier != ProcessInfo.processInfo.processIdentifier ? frontmost : nil
        if panel == nil {
            let state = PaletteState(model: model)
            content = state
            let window = SearchPanel(contentRect: NSRect(x: 0, y: 0, width: 760, height: 500), styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
            window.isFloatingPanel = true
            window.level = .floating
            window.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
            window.isOpaque = false
            window.backgroundColor = .clear
            window.hasShadow = true
            window.isReleasedWhenClosed = false
            window.delegate = self
            window.contentView = NSHostingView(rootView: PaletteView(state: state))
            panel = window
        }
        guard let panel else { return }
        let point = NSEvent.mouseLocation
        let screen = NSScreen.screens.first(where: { $0.frame.contains(point) }) ?? NSScreen.main
        if let frame = screen?.visibleFrame {
            panel.setFrameOrigin(NSPoint(x: frame.midX - 380, y: frame.maxY - panel.frame.height - frame.height * 0.15))
        }
        content?.reset()
        panel.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        content?.focusGeneration += 1
        resignObserver = NotificationCenter.default.addObserver(forName: NSApplication.didResignActiveNotification, object: NSApp, queue: .main) { _ in
            Task { @MainActor in PaletteController.shared.dismiss() }
        }
        outsideMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { _ in
            Task { @MainActor in PaletteController.shared.dismiss() }
        }
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown, .keyDown]) { event in
            let consumed = MainActor.assumeIsolated {
                let controller = PaletteController.shared
                if event.type == .keyDown, event.window === controller.panel {
                    let composing = (controller.panel?.firstResponder as? NSTextInputClient)?.hasMarkedText() ?? false
                    let presented = controller.content?.showActions == true || controller.content?.pendingAction != nil
                    switch PaletteKeyboardRouting.action(key: event.keyCode, modifiers: event.modifierFlags, composing: composing, presentationOpen: presented, actionsOpen: controller.content?.showActions == true && controller.content?.pendingAction == nil) {
                    case .escape: controller.content?.escape(); return true
                    case .down: controller.content?.moveKeyboardSelection(1); return true
                    case .up: controller.content?.moveKeyboardSelection(-1); return true
                    case .pageUp: controller.content?.moveKeyboardSelection(-7); return true
                    case .pageDown: controller.content?.moveKeyboardSelection(7); return true
                    case .first: controller.content?.moveKeyboardSelection(-10000); return true
                    case .last: controller.content?.moveKeyboardSelection(10000); return true
                    case .activate: controller.content?.activateKeyboardSelection(); return true
                    case .actions: controller.content?.showActions.toggle(); return true
                    case nil: break
                    }
                } else if event.type != .keyDown, let window = event.window, !controller.owns(window) { controller.dismiss() }
                return false
            }
            return consumed ? nil : event
        }
    }

    func dismiss() {
        panel?.orderOut(nil)
        content?.cancelSearch()
        content?.revertThemePreview()
        if let outsideMonitor { NSEvent.removeMonitor(outsideMonitor); self.outsideMonitor = nil }
        if let localMonitor { NSEvent.removeMonitor(localMonitor); self.localMonitor = nil }
        if let resignObserver { NotificationCenter.default.removeObserver(resignObserver); self.resignObserver = nil }
    }
    func applyAppearance(light: Bool) { panel?.appearance = NSAppearance(named: light ? .aqua : .darkAqua) }
    private func owns(_ window: NSWindow) -> Bool {
        PaletteWindowOwnership.contains(window, root: panel)
    }
    func windowDidResignKey(_ notification: Notification) {
        // AppKit changes key windows in stages. A sheet/menu transition may
        // briefly have no key window; application deactivation handles that case.
        Task { @MainActor [weak self] in
            await Task.yield()
            guard let self, self.panel?.isVisible == true, let key = NSApp.keyWindow else { return }
            if !self.owns(key) { self.dismiss() }
        }
    }
    func restoreAndPaste() async throws {
        guard let previousApplication, !previousApplication.isTerminated else { dismiss(); return }
        guard AXIsProcessTrusted() else {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "Copied to clipboard. Enable Neko in System Settings → Privacy & Security → Accessibility to paste automatically."])
        }
        dismiss()
        guard previousApplication.activate(options: []) else {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "Copied to clipboard, but the previous application could not be activated. Paste with Command V."])
        }
        for _ in 0..<20 {
            if NSWorkspace.shared.frontmostApplication?.processIdentifier == previousApplication.processIdentifier { break }
            try await Task.sleep(for: .milliseconds(25))
        }
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == previousApplication.processIdentifier else {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "Copied to clipboard. The destination did not become active; paste with Command V."])
        }
        let down = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ANSI_V), keyDown: true)
        let up = CGEvent(keyboardEventSource: nil, virtualKey: CGKeyCode(kVK_ANSI_V), keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.postToPid(previousApplication.processIdentifier)
        up?.postToPid(previousApplication.processIdentifier)
    }
}

@MainActor final class PaletteState: ObservableObject {
    @Published var query = "" { didSet { search() } }
    @Published var rows: [JSONValue] = []
    @Published var selected = 0 { didSet { previewSelectedTheme() } }
    @Published var mode: String?
    @Published var error: String?
    @Published var loading = false
    @Published var showActions = false { didSet { selectedAction = 0 } }
    @Published var selectedAction = 0
    @Published var pendingAction: JSONValue?
    private var pendingItem: JSONValue?
    @Published var activating = false
    @Published var themeID = "neutral"
    @Published var focusGeneration = 0
    private var savedThemeID = "neutral"
    let model: AppModel
    private var searchTask: Task<Void, Never>?
    private let searchTransport: (@Sendable (JSONValue) async -> AsyncThrowingStream<JSONValue, any Error>)?
    private var modeHistory: [(mode: String?, query: String)] = []
    init(model: AppModel, search: (@Sendable (JSONValue) async -> AsyncThrowingStream<JSONValue, any Error>)? = nil) {
        self.model = model
        searchTransport = search
    }
    var selection: JSONValue { rows.indices.contains(selected) ? rows[selected] : .null }
    fileprivate var theme: PaletteTheme { PaletteTheme.named(themeID) }
    func reset() {
        revertThemePreview(); mode = nil; error = nil; showActions = false; pendingAction = nil; pendingItem = nil; rows = []; selected = 0; query = ""
        modeHistory = []
        Task {
            if let response = try? await model.request(.string("GetTheme")) {
                savedThemeID = response["Theme"]["id"].string
                if mode != "theme" { themeID = savedThemeID; PaletteController.shared.applyAppearance(light: theme.light) }
            }
        }
    }
    func revertThemePreview() { themeID = savedThemeID; PaletteController.shared.applyAppearance(light: theme.light) }
    private func previewSelectedTheme() {
        guard mode == "theme", selection != .null else { return }
        themeID = selection["id"].string
        PaletteController.shared.applyAppearance(light: theme.light)
    }
    func cancelSearch() { searchTask?.cancel(); loading = false }
    func move(_ amount: Int) { selected = max(0, min(rows.count - 1, selected + amount)) }
    func moveKeyboardSelection(_ amount: Int) {
        if showActions { selectedAction = max(0, min(selection["actions"].array.count - 1, selectedAction + amount)) }
        else { move(amount) }
    }
    func activateKeyboardSelection() {
        if showActions {
            let actions = selection["actions"].array
            guard actions.indices.contains(selectedAction) else { return }
            activateSelection(action: actions[selectedAction])
        } else { activateSelection() }
    }
    func escape() {
        if showActions { showActions = false }
        else if mode != nil { leaveMode() }
        else { PaletteController.shared.dismiss() }
    }
    func enterMode(_ next: String) {
        modeHistory.append((mode, query))
        mode = next
        query = ""
        showActions = false
    }
    func leaveMode() {
        let previous = modeHistory.popLast() ?? (mode: nil, query: "")
        revertThemePreview()
        mode = previous.mode
        query = previous.query
        focusGeneration += 1
    }
    func search() {
        searchTask?.cancel()
        // A row belongs to the query that produced it. Remove old actionable
        // rows immediately so Enter cannot run an old result with a new query.
        rows = []
        selected = 0
        showActions = false
        loading = true
        let request = PaletteSearchScope.request(query: query, mode: mode)
        searchTask = Task {
            defer { if !Task.isCancelled { loading = false } }
            do {
                let stream: AsyncThrowingStream<JSONValue, any Error>
                if let searchTransport { stream = await searchTransport(request) }
                else { stream = await model.client.responses(request) }
                var first = true
                for try await response in stream {
                    guard !Task.isCancelled else { return }
                    if response["Error"] != .null { error = response["Error"]["message"].string; return }
                    let incoming = response["SearchResults"]["items"].array
                    if first { rows = incoming; selected = 0; first = false }
                    else {
                        // Keep the fast-phase selection and row order stable.
                        let previous = rows
                        rows = previous.map { old in incoming.first(where: { $0["kind"] == old["kind"] && $0["id"] == old["id"] }) ?? old }
                        rows += incoming.filter { new in !previous.contains(where: { $0["kind"] == new["kind"] && $0["id"] == new["id"] }) }
                    }
                    error = nil
                }
            } catch is CancellationError { }
            catch { if !Task.isCancelled { self.error = error.localizedDescription } }
        }
    }
    func activateSelection(action: JSONValue? = nil) {
        guard selection != .null, !activating else { return }
        if let action, action["destructive"].bool { pendingItem = selection; pendingAction = action; return }
        perform(action: action)
    }
    func confirmDestructiveAction() {
        guard let action = pendingAction, let item = pendingItem else { return }
        pendingAction = nil; pendingItem = nil
        perform(action: action, item: item)
    }
    func perform(action: JSONValue? = nil, item selectedItem: JSONValue? = nil) {
        let item = selectedItem ?? selection
        if action == nil, !item["enters_mode"].string.isEmpty {
            switch item["enters_mode"].string {
            case "workspace":
                PaletteController.shared.dismiss(); NotificationCenter.default.post(name: .nekoOpenWorkspace, object: nil); return
            case "preference", "preferences":
                PaletteController.shared.dismiss(); NotificationCenter.default.post(name: .nekoOpenPreferences, object: nil); return
            default: break
            }
            enterMode(item["enters_mode"].string); return
        }
        activating = true
        let queryAtActivation = query
        Task {
            defer { activating = false }
            do {
                _ = try await model.request(.command("Activate", ["kind": item["kind"], "id": item["id"], "action": action?["id"] ?? .null, "query": .string(queryAtActivation)]))
                if item["kind"].string == "theme" { savedThemeID = item["id"].string; themeID = savedThemeID }
                showActions = false
                if action?["id"].string == "delete" || item["keeps_open"].bool { search(); return }
                if item["kind"].string == "clipboard", action == nil || action?["id"].string == "paste" {
                    try await PaletteController.shared.restoreAndPaste()
                } else { PaletteController.shared.dismiss() }
            } catch {
                self.error = error.localizedDescription
                if item["kind"].string == "clipboard" { model.error = error.localizedDescription }
            }
        }
    }
}

private struct PaletteView: View {
    @ObservedObject var state: PaletteState
    @FocusState private var searchFocused: Bool
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                if state.mode != nil { Button { state.leaveMode() } label: { Image(systemName: "chevron.left") }.buttonStyle(.plain).accessibilityLabel("Back to search") }
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
                TextField(state.mode.map { "Search \($0)…" } ?? "Search apps, files, and commands…", text: $state.query)
                    .textFieldStyle(.plain).font(.system(size: 21)).focused($searchFocused)
                if state.loading { ProgressView().controlSize(.small) }
            }.padding(22)
            Divider()
            HStack(spacing: 0) {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 3) {
                            ForEach(Array(state.rows.enumerated()), id: \.offset) { index, row in
                                if index == 0 || state.rows[index - 1]["section_label"] != row["section_label"] {
                                    Text(row["section_label"].string).font(.caption.weight(.semibold)).foregroundStyle(.secondary).frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 10).padding(.top, 8)
                                }
                                Button { state.selected = index; state.activateSelection() } label: {
                                    HStack(spacing: 12) {
                                        PaletteIcon(icon: row["icon"]).frame(width: 24, height: 24)
                                        VStack(alignment: .leading, spacing: 3) {
                                            Text(row["title"].string).lineLimit(1)
                                            if !row["subtitle"].string.isEmpty { Text(row["subtitle"].string).font(.caption).foregroundStyle(.secondary).lineLimit(1) }
                                            if row["meter"] != .null { PaletteMeter(meter: row["meter"]) }
                                        }
                                        Spacer(minLength: 0)
                                        Text(row["accessory"].string).font(.caption).foregroundStyle(.secondary)
                                    }.padding(10).contentShape(Rectangle()).background(index == state.selected ? state.theme.selected : .clear, in: RoundedRectangle(cornerRadius: 8))
                                }.buttonStyle(.plain).id(index)
                            }
                            if state.rows.isEmpty, !state.loading { Text("No results").foregroundStyle(.secondary).padding(32) }
                        }.padding(10)
                    }.onChange(of: state.selected) { _, value in proxy.scrollTo(value) }
                }
                if state.mode != nil, PalettePresentation.hasDetail(state.selection) {
                    Divider()
                    PaletteDetail(row: state.selection).frame(width: 330)
                }
            }
            if let error = state.error { Text(error).font(.caption).foregroundStyle(.red).textSelection(.enabled).padding(10).frame(maxWidth: .infinity, alignment: .leading) }
            Divider()
            HStack {
                Menu {
                    Button("Open workspace") { PaletteController.shared.dismiss(); NotificationCenter.default.post(name: .nekoOpenWorkspace, object: nil) }
                    Button("Preferences…") { PaletteController.shared.dismiss(); NotificationCenter.default.post(name: .nekoOpenPreferences, object: nil) }
                } label: { Label("Neko", systemImage: "sparkle") }.menuStyle(.borderlessButton).fixedSize()
                Spacer()
                Text(state.selection["action_label"].string).font(.caption).foregroundStyle(.secondary)
                Button("Actions ⌘K") { state.showActions.toggle() }.disabled(state.selection["actions"].array.isEmpty)
            }.padding(12)
        }.frame(width: 760, height: 500).foregroundStyle(state.theme.text)
            .background(state.theme.surface.opacity(state.theme.alpha))
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16)).clipShape(RoundedRectangle(cornerRadius: 16))
            .preferredColorScheme(state.theme.light ? .light : .dark)
            .onAppear { searchFocused = true }
            .onChange(of: state.focusGeneration) { _, _ in searchFocused = true }
            .overlay(alignment: .bottomTrailing) {
                if state.showActions {
                    ZStack(alignment: .bottomTrailing) {
                        Color.clear.contentShape(Rectangle()).onTapGesture { state.showActions = false }
                        VStack(alignment: .leading, spacing: 8) {
                            ForEach(Array(state.selection["actions"].array.enumerated()), id: \.offset) { index, action in
                                Button(role: action["destructive"].bool ? .destructive : nil) { state.activateSelection(action: action) } label: {
                                    Text(action["label"].string).frame(maxWidth: .infinity, alignment: .leading).padding(6).background(state.selectedAction == index ? state.theme.selected : .clear, in: RoundedRectangle(cornerRadius: 6))
                                }.buttonStyle(.plain)
                            }
                            Divider()
                            Button("Close actions") { state.showActions = false }.keyboardShortcut(.escape, modifiers: [])
                        }.padding(12).frame(width: 210).background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12)).shadow(radius: 12).padding(.trailing, 12).padding(.bottom, 50)
                    }
                }
            }
            .alert("Delete this item?", isPresented: Binding(get: { state.pendingAction != nil }, set: { if !$0 { state.pendingAction = nil } })) {
                Button("Delete", role: .destructive) { state.confirmDestructiveAction() }
                Button("Cancel", role: .cancel) { state.pendingAction = nil }
            } message: { Text("This removes the item from clipboard history permanently.") }
    }
}

private struct PaletteMeter: View {
    let meter: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            ProgressView(value: PalettePresentation.meterFraction(meter)).accessibilityLabel("Usage")
            HStack(alignment: .top, spacing: 12) {
                ForEach(Array(meter["stats"].array.enumerated()), id: \.offset) { _, stat in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(stat["label"].string).font(.caption2).foregroundStyle(.secondary)
                        Text(stat["value"].string).font(.caption.weight(.medium))
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }.padding(.top, 4)
    }
}

private struct PaletteDetail: View {
    let row: JSONValue
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                if !row["speaker"].string.isEmpty { Text(row["speaker"].string.capitalized).font(.caption.weight(.semibold)).foregroundStyle(.secondary) }
                let preview = PalettePresentation.preview(row)
                if row["preview_markdown"].bool {
                    Text((try? AttributedString(markdown: preview, options: .init(interpretedSyntax: .full))) ?? AttributedString(preview)).textSelection(.enabled)
                } else { Text(preview).textSelection(.enabled) }
                ForEach(Array(row["images"].array.enumerated()), id: \.offset) { _, image in
                    if let picture = NSImage(contentsOfFile: image.string) {
                        Image(nsImage: picture).resizable().scaledToFit().accessibilityLabel("Attached image")
                    } else { Label("Image unavailable", systemImage: "photo").foregroundStyle(.secondary) }
                }
                if row["kind"].string == "clipboard" {
                    Divider()
                    if !row["source"].string.isEmpty { LabeledContent("Application", value: row["source"].string) }
                    if !row["badge"].string.isEmpty { LabeledContent("Content Type", value: row["badge"].string.capitalized) }
                    if !row["accessory"].string.isEmpty { LabeledContent("Copied", value: row["accessory"].string) }
                } else {
                    if !row["source"].string.isEmpty { Divider(); Text("Source: " + row["source"].string).font(.caption).foregroundStyle(.secondary) }
                    if !row["badge"].string.isEmpty { Text(row["badge"].string).font(.caption2).foregroundStyle(.secondary) }
                }
            }.frame(maxWidth: .infinity, alignment: .topLeading).padding(18)
        }
    }
}

private struct PaletteIcon: View {
    let icon: JSONValue
    var body: some View {
        if let image = NSImage(contentsOfFile: icon["Image"].string) {
            Image(nsImage: image).resizable().scaledToFit()
        } else {
            Image(systemName: ["Text": "text.alignleft", "Link": "link", "File": "doc", "Folder": "folder", "Clipboard": "doc.on.clipboard", "Agent": "terminal", "AgentLive": "terminal.fill", "Sliders": "slider.horizontal.3", "Palette": "paintpalette"][icon["Glyph"].string] ?? "square.grid.2x2").foregroundStyle(.secondary)
        }
    }
}

/// These surface/text values mirror the shipped tokens in Rust theme.rs.
/// Native controls and material still use AppKit's light/dark appearance.
private struct PaletteTheme {
    let light: Bool
    let surface: Color
    let alpha: Double
    let selected: Color
    let text: Color
    static func named(_ id: String) -> PaletteTheme {
        let values: [String: (Bool, UInt32, Double, UInt32, UInt32)] = [
            "neutral": (false, 0x0d0d0d, 0.64, 0x2f2f2f, 0xe8e8e8),
            "ember": (false, 0x350915, 0.86, 0x7b3820, 0xf9eee0),
            "catnap": (false, 0x4b115b, 0.9, 0xa0186f, 0xfff2fd),
            "catppuccin-mocha": (false, 0x1e1e2e, 0.84, 0x45475a, 0xcdd6f4),
            "catppuccin-macchiato": (false, 0x24273a, 0.84, 0x494d64, 0xcad3f5),
            "catppuccin-frappe": (false, 0x303446, 0.86, 0x51576d, 0xc6d0f5),
            "catppuccin-latte": (true, 0xeff1f5, 0.88, 0xccd0da, 0x4c4f69),
            "gruvbox-dark": (false, 0x282828, 0.85, 0x504945, 0xfbf1c7),
            "gruvbox-light": (true, 0xfbf1c7, 0.9, 0xd5c4a1, 0x282828),
            "solarized-dark": (false, 0x002b36, 0.86, 0x0d4553, 0xfdf6e3),
            "solarized-light": (true, 0xfdf6e3, 0.9, 0xd9d2ba, 0x002b36),
            "nord": (false, 0x2e3440, 0.85, 0x434c5e, 0xeceff4),
            "tokyo-night": (false, 0x24283b, 0.85, 0x3b4261, 0xc0caf5),
            "rose-pine": (false, 0x191724, 0.86, 0x403d52, 0xe0def4),
            "rose-pine-dawn": (true, 0xfaf4ed, 0.9, 0xcecacd, 0x555076),
            "dracula": (false, 0x282a36, 0.85, 0x44475a, 0xf8f8f2),
            "everforest-dark": (false, 0x2d353b, 0.86, 0x3d484d, 0xd3c6aa)
        ]
        let value = values[id] ?? values["neutral"]!
        func color(_ hex: UInt32) -> Color { Color(red: Double((hex >> 16) & 255) / 255, green: Double((hex >> 8) & 255) / 255, blue: Double(hex & 255) / 255) }
        return PaletteTheme(light: value.0, surface: color(value.1), alpha: value.2, selected: color(value.3), text: color(value.4))
    }
}
