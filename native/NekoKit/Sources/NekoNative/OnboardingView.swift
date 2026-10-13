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
    @State private var registrySheet = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        VStack(spacing: NekoLayout.sectionGap) {
            ScrollView {
                VStack(spacing: NekoLayout.sectionGap) {
                    Group {
                        switch step {
                        case 0: welcomeStep
                        case 1: permissionsStep
                        default: workspaceStep
                        }
                    }
                    .transition(.opacity)
                    .id(step)
                    if let error = model.error {
                        Text(error).font(NekoFont.body).foregroundStyle(NekoStyle.coral)
                            .textSelection(.enabled).padding(.top, NekoSpace.md)
                    }
                }
                .frame(maxWidth: NekoLayout.readingWidth).padding(.vertical, NekoSpace.xxl).frame(maxWidth: .infinity)
            }
            HStack {
                if step > 0 { Button("Back") { step -= 1 }.nekoButton(.secondary) }
                Spacer()
                Text("Step \(step + 1) of 3").font(NekoFont.meta).foregroundStyle(.secondary)
                Spacer()
                if step > 0 {
                    if step < 2 { Button("Skip") { finish() }.buttonStyle(.plain).foregroundStyle(N.text3) }
                    Button(step == 2 ? "Open Neko" : "Continue") { if step == 2 { finish() } else { step += 1 } }.nekoButton(.primary)
                } else { Color.clear.frame(width: 60, height: 1) }
            }
            .disabled(pending)
            .frame(maxWidth: NekoLayout.readingWidth)
        }
        .font(NekoFont.body).controlSize(.regular)
        .padding(NekoLayout.pageInset)
        .frame(width: 840, height: 660)
        .background(N.canvas)
        .hiddenWindowToolbarBackground()
        .navigationTitle("")
            .nekoAnimation(.reveal, value: step)
            .sheet(isPresented: $workspaceSheet) { WorkspaceEditor(model: model) }
            .sheet(isPresented: $registrySheet) { MCPRegistryBrowser(model: model) }
            .task {
                do { clipboard = (try await model.request(.string("GetClipboardHistoryEnabled")))["ClipboardHistoryEnabled"]["enabled"].bool }
                catch { model.error = error.localizedDescription }
                while !Task.isCancelled {
                    trusted = AXIsProcessTrusted()
                    try? await Task.sleep(for: .seconds(1))
                }
            }
    }
    private var welcomeStep: some View {
        VStack(spacing: NekoLayout.sectionGap) {
            BrandMark(size: 76).clipShape(RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
            VStack(spacing: 10) {
                Text("A little more off your mind.").font(NekoFont.display).multilineTextAlignment(.center).foregroundStyle(N.text)
                    .accessibilityAddTraits(.isHeader)
                Text("Connect your projects and tools, hand off work, and see what needs your attention.")
                    .font(NekoFont.body).foregroundStyle(N.text3).multilineTextAlignment(.center).frame(maxWidth: 480)
            }
            Button("Get started", systemImage: "arrow.right") { step = 1 }
                .nekoButton(.primary).controlSize(.large).padding(.top, NekoSpace.sm)
        }
    }
    private var permissionsStep: some View {
        stepCard(title: "Let Neko work on your Mac", subtitle: "Each of these is optional. You can change them later in Settings.") {
            settingRow(symbol: "hand.point.up.left.fill", title: "Accessibility", detail: "Lets Neko paste for you. macOS asks you to confirm.") {
                Button(trusted ? "Enabled" : "Allow…") {
                    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
                    trusted = AXIsProcessTrustedWithOptions(options)
                }.disabled(trusted).nekoButton(.secondary)
            }
            Divider()
            settingRow(symbol: "doc.on.clipboard.fill", title: "Clipboard history", detail: "Saved only on this Mac. Off by default.") {
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
                .toggleStyle(.switch).labelsHidden()
            }
            Divider()
            settingRow(symbol: "keyboard.fill", title: "Quick panel shortcut", detail: "Summon Neko from anywhere.") {
                HotkeySettingsView(model: model)
            }
        }
    }
    private var workspaceStep: some View {
        VStack(spacing: NekoLayout.sectionGap) {
        stepCard(title: "Where do you work?", subtitle: "Choose your folders. Each workspace keeps its own tools and instructions.") {
            if model.homeWorkspaceID == nil {
                Button { Task { await model.addHomeWorkspace() } } label: {
                    NekoRow(symbol: "house", title: "Use my home folder", detail: "Recommended · one default workspace for everything in ~") {
                        Image(systemName: "chevron.right").foregroundStyle(N.text3)
                    }.padding(.vertical, NekoLayout.rowInset).contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
            if model.workspaces.isEmpty {
                Button { model.selectedWorkspace = nil; workspaceSheet = true } label: {
                    NekoRow(symbol: "folder.badge.plus", title: "Choose specific folders", detail: "A project with its own tools and instructions") {
                        Image(systemName: "chevron.right").foregroundStyle(N.text3)
                    }.padding(.vertical, NekoLayout.rowInset).contentShape(Rectangle())
                }.buttonStyle(.plain)
            } else {
                ForEach(model.workspaces, id: \.recordID) { item in
                    HStack(spacing: 10) {
                        Image(systemName: "folder").foregroundStyle(N.text3)
                        Text(item["name"].string).font(NekoFont.heading).foregroundStyle(N.text)
                        Spacer()
                        Image(systemName: "checkmark.circle.fill").foregroundStyle(NekoStyle.mint)
                    }.padding(.vertical, NekoSpace.md)
                }
                Button("Add another workspace…") { model.selectedWorkspace = nil; workspaceSheet = true }.nekoButton(.secondary)
            }
        }
        Button { registrySheet = true } label: {
            Label("Find Sentry and other MCP servers", systemImage: "network")
                .font(NekoFont.body)
        }
        .buttonStyle(.plain)
        .foregroundStyle(NekoStyle.accent)
        .accessibilityHint("Browse hosted connections in the public MCP Registry")
        }
    }
    private func stepCard<Content: View>(title: String, subtitle: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
            VStack(alignment: .leading, spacing: 10) {
                Text(title).font(NekoFont.display).foregroundStyle(N.text).accessibilityAddTraits(.isHeader)
                Text(subtitle).font(NekoFont.body).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: 0, content: content)
        }.frame(maxWidth: NekoLayout.readingWidth, alignment: .leading)
    }
    private func settingRow<Trailing: View>(symbol: String, title: String, detail: String, @ViewBuilder trailing: @escaping () -> Trailing) -> some View {
        NekoRow(symbol: symbol, title: title, detail: detail, control: trailing).padding(.vertical, NekoLayout.rowInset)
    }
    private func finish() { pending = true; Task { await model.completeSetup(); pending = false } }
}
