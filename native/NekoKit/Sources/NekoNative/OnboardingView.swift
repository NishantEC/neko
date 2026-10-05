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
                            .textSelection(.enabled).padding(.top, 12)
                    }
                }
                .frame(maxWidth: NekoLayout.readingWidth).padding(.vertical, 24).frame(maxWidth: .infinity)
            }
            HStack {
                if step > 0 { Button("Back") { step -= 1 }.buttonStyle(.bordered) }
                Spacer()
                Text("Step \(step + 1) of 3").font(NekoFont.meta).foregroundStyle(.secondary)
                Spacer()
                if step > 0 {
                    if step < 2 { Button("Skip") { finish() }.buttonStyle(.plain).foregroundStyle(N.text3) }
                    Button(step == 2 ? "Open Neko" : "Continue") { if step == 2 { finish() } else { step += 1 } }.buttonStyle(.borderedProminent)
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
            .animation(reduceMotion ? nil : .easeInOut(duration: 0.18), value: step)
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
            BrandMark(size: 76).clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
            VStack(spacing: 10) {
                Text("A little more off your mind.").font(NekoFont.display).multilineTextAlignment(.center).foregroundStyle(N.text)
                    .accessibilityAddTraits(.isHeader)
                Text("Connect your projects and tools, hand off work, and see what needs your attention.")
                    .font(NekoFont.body).foregroundStyle(N.text3).multilineTextAlignment(.center).frame(maxWidth: 480)
            }
            Button("Get started", systemImage: "arrow.right") { step = 1 }
                .buttonStyle(.borderedProminent).controlSize(.large).padding(.top, 8)
        }
    }
    private var permissionsStep: some View {
        stepCard(title: "Let Neko work on your Mac", subtitle: "Each of these is optional. You can change them later in Settings.") {
            settingRow(symbol: "hand.point.up.left.fill", tint: Gem.sapphire, title: "Accessibility", detail: "Lets Neko paste for you. macOS asks you to confirm.") {
                Button(trusted ? "Enabled" : "Allow…") {
                    let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
                    trusted = AXIsProcessTrustedWithOptions(options)
                }.disabled(trusted).buttonStyle(.bordered)
            }
            Divider()
            settingRow(symbol: "doc.on.clipboard.fill", tint: Gem.emerald, title: "Clipboard history", detail: "Saved only on this Mac. Off by default.") {
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
            settingRow(symbol: "keyboard.fill", tint: Gem.amethyst, title: "Quick panel shortcut", detail: "Summon Neko from anywhere.") {
                HotkeySettingsView(model: model)
            }
        }
    }
    private var workspaceStep: some View {
        VStack(spacing: NekoLayout.sectionGap) {
        stepCard(title: "Where do you work?", subtitle: "Choose your folders. Each workspace keeps its own tools and instructions.") {
            if model.homeWorkspaceID == nil {
                Button { Task { await model.addHomeWorkspace() } } label: {
                    HStack(spacing: 14) {
                        setupIcon("house", tint: NekoStyle.accent)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Use my home folder").font(NekoFont.heading).foregroundStyle(N.text)
                            Text("Recommended · one default workspace for everything in ~").font(NekoFont.meta).foregroundStyle(N.text3)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").foregroundStyle(N.text3)
                    }.padding(.vertical, NekoLayout.rowInset).contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
            if model.workspaces.isEmpty {
                Button { model.selectedWorkspace = nil; workspaceSheet = true } label: {
                    HStack(spacing: 14) {
                        setupIcon("folder.badge.plus", tint: NekoStyle.accent)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Choose specific folders").font(NekoFont.heading).foregroundStyle(N.text)
                            Text("A project with its own tools and instructions").font(NekoFont.body).foregroundStyle(N.text3)
                        }
                        Spacer()
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
                    }.padding(.vertical, 12)
                }
                Button("Add another workspace…") { model.selectedWorkspace = nil; workspaceSheet = true }.buttonStyle(.bordered)
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
    private func settingRow<Trailing: View>(symbol: String, tint: Color, title: String, detail: String, @ViewBuilder trailing: () -> Trailing) -> some View {
        HStack(spacing: 18) {
            setupIcon(symbol, tint: NekoStyle.accent)
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(NekoFont.heading).foregroundStyle(N.text)
                Text(detail).font(NekoFont.meta).foregroundStyle(N.text3)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            trailing()
        }.padding(.vertical, NekoLayout.rowInset)
    }
    private func setupIcon(_ symbol: String, tint: Color) -> some View {
        Image(systemName: symbol).font(.system(size: 20, weight: .regular))
            .foregroundStyle(tint).frame(width: 40, height: 40)
            .background(N.selected, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .accessibilityHidden(true)
    }
    private func finish() { pending = true; Task { await model.completeSetup(); pending = false } }
}
