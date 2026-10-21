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
    private var stopHotkey: EventHotKeyRef?
    private var handler: EventHandlerRef?
    private var registeredKey: UInt32?
    private var registeredModifiers: UInt32?
    private var outsideMonitor: Any?
    private var localMonitor: Any?
    private var resignObserver: NSObjectProtocol?
    /// Tokens for each show and hide, so late callbacks from an earlier one do nothing.
    private var presentation = PaletteGeneration()
    /// True from show until dismissal starts (the panel stays visible while it fades).
    private var shown = false

    func configure(model: AppModel) async {
        self.model = model
        // Build the panel now, off screen, so the first summon has nothing to construct.
        preparePanel(model: model)
        NativeHotkeySettings.register = { [weak self] combo in
            guard let self else { throw DaemonClientError.disconnected }
            try self.register(combo: combo)
        }
        if handler == nil {
            var event = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
            InstallEventHandler(GetApplicationEventTarget(), { _, event, _ in
                var pressed = EventHotKeyID()
                GetEventParameter(event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil, MemoryLayout<EventHotKeyID>.size, nil, &pressed)
                let id = pressed.id
                Task { @MainActor in
                    let controller = PaletteController.shared
                    guard let model = controller.model else { return }
                    if id == StopAllWork.hotkeyID { await StopAllWork.run(model) } else { controller.toggle(model: model) }
                }
                return noErr
            }, 1, &event, nil, &handler)
        }
        if stopHotkey == nil {
            // ⌘⇧Esc: stop everything from anywhere. Not a system shortcut (Force Quit is ⌘⌥Esc).
            RegisterEventHotKey(UInt32(53), UInt32(cmdKey | shiftKey), EventHotKeyID(signature: 0x4E454B4F, id: StopAllWork.hotkeyID), GetApplicationEventTarget(), 0, &stopHotkey)
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

    private func preparePanel(model: AppModel) {
        guard panel == nil else { return }
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

    func toggle(model: AppModel) {
        if shown { dismiss(); return }
        self.model = model
        let frontmost = NSWorkspace.shared.frontmostApplication
        previousApplication = frontmost?.processIdentifier != ProcessInfo.processInfo.processIdentifier ? frontmost : nil
        preparePanel(model: model)
        guard let panel else { return }
        let token = presentation.advance()
        shown = true
        panel.alphaValue = 1
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
            Task { @MainActor in PaletteController.shared.dismiss(ifCurrent: token) }
        }
        outsideMonitor = NSEvent.addGlobalMonitorForEvents(matching: [.leftMouseDown, .rightMouseDown]) { _ in
            Task { @MainActor in PaletteController.shared.dismiss(ifCurrent: token) }
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

    /// Dismiss only if nothing has been shown or hidden since `token`.
    func dismiss(ifCurrent token: Int) { if presentation.isCurrent(token) { dismiss() } }

    func dismiss() {
        guard shown else { return }
        shown = false
        let token = presentation.advance()
        if let panel {
            // A short fade, or none under Reduce Motion. The panel is only
            // ordered out if nothing newer has started in the meantime.
            if NekoMotion.systemReducesMotion { panel.orderOut(nil) } else {
                NSAnimationContext.runAnimationGroup({ context in
                    context.duration = 0.08
                    panel.animator().alphaValue = 0
                }, completionHandler: {
                    Task { @MainActor in
                        let controller = PaletteController.shared
                        guard controller.presentation.isCurrent(token) else { return }
                        panel.orderOut(nil); panel.alphaValue = 1
                    }
                })
            }
        }
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
        let token = presentation.generation
        Task { @MainActor [weak self] in
            await Task.yield()
            guard let self, self.shown, let key = NSApp.keyWindow else { return }
            if !self.owns(key) { self.dismiss(ifCurrent: token) }
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
            HStack(spacing: NekoSpace.md) {
                if state.mode != nil { Button { state.leaveMode() } label: { Image(systemName: "chevron.left") }.buttonStyle(.plain).accessibilityLabel("Back to search") }
                Image(systemName: "magnifyingglass").foregroundStyle(.secondary).accessibilityHidden(true)
                TextField(state.mode.map { "Search \($0)…" } ?? "Search apps, files, and commands…", text: $state.query)
                    .textFieldStyle(.plain).font(NekoFont.of(.subtitle)).focused($searchFocused)
                if state.loading { ProgressView().controlSize(.small).accessibilityLabel("Searching") }
            }
            .padding(.horizontal, NekoSpace.xl).padding(.vertical, NekoSpace.lg)
            Divider()
            HStack(spacing: 0) {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: NekoSpace.hair) {
                            ForEach(Array(state.rows.enumerated()), id: \.offset) { index, row in
                                if index == 0 || state.rows[index - 1]["section_label"] != row["section_label"] {
                                    Text(row["section_label"].string).font(NekoFont.label).foregroundStyle(.secondary)
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(.horizontal, NekoSpace.md).padding(.top, NekoSpace.md).padding(.bottom, NekoSpace.xxs)
                                        .accessibilityAddTraits(.isHeader)
                                }
                                Button { state.selected = index; state.activateSelection() } label: { rowView(row, selected: index == state.selected) }
                                    .buttonStyle(.plain).id(index)
                                    .accessibilityLabel(rowLabel(row))
                                    .accessibilityAddTraits(index == state.selected ? .isSelected : [])
                            }
                            if state.rows.isEmpty, !state.loading, !state.query.isEmpty {
                                NekoEmptyState(reason: .noMatches(state.query), block: true)
                            }
                        }.padding(NekoSpace.sm)
                    }.onChange(of: state.selected) { _, value in proxy.scrollTo(value) }
                }
                if state.mode != nil, PalettePresentation.hasDetail(state.selection) {
                    Divider()
                    PaletteDetail(row: state.selection).frame(width: 330)
                }
            }
            if let error = state.error {
                Label(error, systemImage: "exclamationmark.triangle.fill").font(NekoFont.caption).foregroundStyle(NekoStyle.coral)
                    .textSelection(.enabled)
                    .padding(.horizontal, NekoSpace.md).padding(.vertical, NekoSpace.sm)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider()
            footer
        }.frame(width: NekoLayout.paletteWidth, height: NekoLayout.paletteHeight).foregroundStyle(state.theme.text)
            .background(state.theme.surface.opacity(state.theme.alpha))
            .liquidGlass(radius: NekoRadius.xl).clipShape(RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
            .preferredColorScheme(state.theme.light ? .light : .dark)
            .onAppear { searchFocused = true }
            .onChange(of: state.focusGeneration) { _, _ in searchFocused = true }
            .overlay(alignment: .bottomTrailing) {
                if state.showActions {
                    ZStack(alignment: .bottomTrailing) {
                        Color.clear.contentShape(Rectangle()).onTapGesture { state.showActions = false }
                        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                            ForEach(Array(state.selection["actions"].array.enumerated()), id: \.offset) { index, action in
                                Button(role: action["destructive"].bool ? .destructive : nil) { state.activateSelection(action: action) } label: {
                                    Text(action["label"].string).frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(.horizontal, NekoSpace.xs).padding(.vertical, NekoSpace.xxs)
                                        .background(state.selectedAction == index ? state.theme.selected : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.sm))
                                }
                                .buttonStyle(.plain)
                                .accessibilityAddTraits(state.selectedAction == index ? .isSelected : [])
                            }
                            Divider()
                            Button("Close actions") { state.showActions = false }.keyboardShortcut(.escape, modifiers: [])
                        }.padding(NekoSpace.md).frame(width: 210).liquidGlass(radius: NekoRadius.lg).nekoElevation(.floating)
                            .nekoEntrance()
                            .padding(.trailing, NekoSpace.md).padding(.bottom, NekoLayout.paletteBarHeight + NekoSpace.sm)
                    }
                }
            }
            .alert("Delete from clipboard history?", isPresented: Binding(get: { state.pendingAction != nil }, set: { if !$0 { state.pendingAction = nil } })) {
                Button("Delete item", role: .destructive) { state.confirmDestructiveAction() }
                Button("Cancel", role: .cancel) { state.pendingAction = nil }
            } message: { Text("This removes the item from clipboard history permanently.") }
    }

    /// One line, as Raycast lays it out: icon, title, the subtitle inline
    /// after it, and the accessory on the right. A meter adds its own line.
    private func rowView(_ row: JSONValue, selected: Bool) -> some View {
        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
            HStack(spacing: NekoSpace.md) {
                PaletteIcon(icon: row["icon"]).frame(width: NekoLayout.listGlyphSlot, height: NekoLayout.listGlyphSlot).accessibilityHidden(true)
                HStack(alignment: .firstTextBaseline, spacing: NekoSpace.sm) {
                    Text(row["title"].string).font(NekoFont.of(.callout)).lineLimit(1)
                    if !row["subtitle"].string.isEmpty {
                        Text(row["subtitle"].string).font(NekoFont.meta).foregroundStyle(.secondary).lineLimit(1).layoutPriority(-1)
                    }
                }
                Spacer(minLength: 0)
                Text(row["accessory"].string).font(NekoFont.caption).foregroundStyle(.secondary).lineLimit(1)
            }
            if row["meter"] != .null { PaletteMeter(meter: row["meter"]).padding(.leading, NekoLayout.listGlyphSlot + NekoSpace.md) }
        }
        .padding(.horizontal, NekoSpace.md).padding(.vertical, NekoSpace.sm)
        .frame(minHeight: NekoLayout.paletteRowHeight)
        .contentShape(Rectangle())
        .background(selected ? state.theme.selected : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous))
        .help([row["title"].string, row["subtitle"].string].filter { !$0.isEmpty }.joined(separator: " — "))
    }

    /// The whole row for VoiceOver, including the subtitle that truncates first.
    private func rowLabel(_ row: JSONValue) -> String {
        [row["title"].string, row["subtitle"].string, row["accessory"].string].filter { !$0.isEmpty }.joined(separator: ", ")
    }

    /// The menu on the left; on the right, what Return does and how to see more.
    private var footer: some View {
        HStack(spacing: NekoSpace.sm) {
            Menu {
                Button("Open workspace") { PaletteController.shared.dismiss(); NotificationCenter.default.post(name: .nekoOpenWorkspace, object: nil) }
                Button("Preferences…") { PaletteController.shared.dismiss(); NotificationCenter.default.post(name: .nekoOpenPreferences, object: nil) }
            } label: {
                Label { Text("Neko") } icon: { BrandMark(size: NekoSpace.lg) }
            }.menuStyle(.borderlessButton).fixedSize()
            Spacer()
            let verb = state.selection["action_label"].string
            if !verb.isEmpty {
                HStack(spacing: NekoSpace.xs) {
                    Text(verb).font(NekoFont.caption).foregroundStyle(.secondary)
                    NekoKeycap("↵")
                }
                .accessibilityElement(children: .ignore).accessibilityLabel("\(verb), Return")
                Hairline(vertical: true).frame(height: NekoSpace.lg)
            }
            Button { state.showActions.toggle() } label: {
                HStack(spacing: NekoSpace.xxs) {
                    Text("Actions").font(NekoFont.caption)
                    NekoKeycap("⌘")
                    NekoKeycap("K")
                }
            }
            .nekoButton(.quiet)
            .disabled(state.selection["actions"].array.isEmpty)
            .accessibilityLabel("Actions, Command K")
        }
        .padding(.horizontal, NekoSpace.md)
        .frame(height: NekoLayout.paletteBarHeight)
    }
}

private struct PaletteMeter: View {
    let meter: JSONValue
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
            ProgressView(value: PalettePresentation.meterFraction(meter)).accessibilityLabel("Usage")
            HStack(alignment: .top, spacing: NekoSpace.md) {
                ForEach(Array(meter["stats"].array.enumerated()), id: \.offset) { _, stat in
                    VStack(alignment: .leading, spacing: NekoSpace.hair) {
                        Text(stat["label"].string).font(NekoFont.of(.small)).foregroundStyle(.secondary)
                        Text(stat["value"].string).font(NekoFont.of(.caption, weight: .medium))
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }.padding(.top, NekoSpace.xxs)
    }
}

private struct PaletteDetail: View {
    let row: JSONValue
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: NekoSpace.md) {
                if !row["speaker"].string.isEmpty { Text(row["speaker"].string.capitalized).font(NekoFont.of(.caption, weight: .semibold)).foregroundStyle(.secondary) }
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
                    if !row["source"].string.isEmpty { Divider(); Text("Source: " + row["source"].string).font(NekoFont.caption).foregroundStyle(.secondary) }
                    if !row["badge"].string.isEmpty { Text(row["badge"].string).font(NekoFont.of(.small)).foregroundStyle(.secondary) }
                }
            }.frame(maxWidth: .infinity, alignment: .topLeading).padding(NekoSpace.lg)
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
