import SwiftUI
@preconcurrency import ApplicationServices
import NekoKit

struct OnboardingView: View {
    @ObservedObject var model: AppModel
    @State private var step = 0
    @State private var trusted = AXIsProcessTrusted()
    @State private var clipboard: Bool?
    @State private var pending = false
    @State private var workspaceSheet = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            HStack { Text("Neko").font(.headline); Spacer(); Text("Setup · \(step + 1) of 3").foregroundStyle(.secondary) }
            Spacer()
            switch step {
            case 0:
                BrandMark(size: 96).clipShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
                    .shadow(color: NekoStyle.accent.opacity(0.35), radius: 28, y: 12)
                OpalGreeting(title: "A little less on\nyour mind.", subtitle: "One Neko to think with. A quick panel when you need it. Your work stays yours.")
                Button("Get started") { step = 1 }.glassProminentButton().tint(NekoStyle.accent).controlSize(.large)
            case 1:
                Text("Let Neko work on your Mac").font(.largeTitle.bold())
                Text("Each permission is optional. You can always open the full app without a shortcut.").foregroundStyle(.secondary)
                Button(trusted ? "Accessibility enabled" : "Allow Accessibility…") {
                    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
                    trusted = AXIsProcessTrustedWithOptions(options)
                }.disabled(trusted)
                Text("Used for paste actions. macOS controls this permission.").font(.caption)
                Toggle("Clipboard history", isOn: Binding(get: { clipboard ?? false }, set: { value in
                    pending = true
                    Task {
                        do {
                            let reply = try await model.request(.command("SetClipboardHistoryEnabled", ["enabled": .bool(value)]))
                            guard reply["ClipboardHistoryEnabled"]["enabled"] == .bool(value) else { throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "Clipboard setting was not acknowledged."]) }
                            clipboard = value; model.error = nil
                        } catch { model.error = error.localizedDescription }
                        pending = false
                    }
                })).disabled(clipboard == nil || pending)
                Text("Saved locally. Off by default; enabling starts clipboard capture.").font(.caption)
                HotkeySettingsView(model: model)
            default:
                Text("Choose a workspace").font(.largeTitle.bold())
                Text("Work where you already work. Choose an existing workspace or add a folder.").foregroundStyle(.secondary)
                Picker("Workspace", selection: $model.selectedWorkspace) {
                    Text("Choose later").tag(String?.none)
                    ForEach(model.workspaces, id: \.recordID) { item in Text(item["name"].string).tag(Optional(item.recordID)) }
                }
                Button("Add workspace…") { model.selectedWorkspace = nil; workspaceSheet = true }
            }
            if let error = model.error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
            Spacer()
            if step > 0 {
                HStack {
                    Button("Back") { step -= 1 }
                    Spacer()
                    if step < 2 { Button("Set up later") { finish() } }
                    Button(step == 2 ? "Open Neko" : "Continue") { if step == 2 { finish() } else { step += 1 } }.nekoPrimaryButton()
                }.disabled(pending)
            }
        }.padding(48).frame(minWidth: 650, minHeight: 530).frame(maxWidth: .infinity, maxHeight: .infinity).background { LookBackground(look: "ambient").ignoresSafeArea() }
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: step)
            .sheet(isPresented: $workspaceSheet) { WorkspaceEditor(model: model) }
            .task {
                do { clipboard = (try await model.request(.string("GetClipboardHistoryEnabled")))["ClipboardHistoryEnabled"]["enabled"].bool }
                catch { model.error = error.localizedDescription }
                while !Task.isCancelled {
                    trusted = AXIsProcessTrusted()
                    try? await Task.sleep(for: .seconds(1))
                }
            }
    }
    private func finish() { pending = true; Task { await model.completeSetup(); pending = false } }
}
