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
        VStack(spacing: 24) {
            ScrollView {
                VStack(spacing: 16) {
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
                .frame(maxWidth: 560).padding(.vertical, 20).frame(maxWidth: .infinity)
            }
            HStack {
                if step > 0 { Button("Back") { step -= 1 }.buttonStyle(.bordered) }
                Spacer()
                Text("Step \(step + 1) of 3").font(NekoFont.meta).foregroundStyle(.secondary)
                Spacer()
                if step > 0 {
                    if step < 2 { Button("Skip") { finish() }.buttonStyle(.plain).foregroundStyle(.white.opacity(0.7)) }
                    Button(step == 2 ? "Open Neko" : "Continue") { if step == 2 { finish() } else { step += 1 } }.buttonStyle(.borderedProminent)
                } else { Color.clear.frame(width: 60, height: 1) }
            }
            .disabled(pending)
            .frame(maxWidth: 560)
        }
        .padding(32)
        .frame(width: 760, height: 580)
        .background(N.canvas)
        .environment(\.colorScheme, .dark)
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
        VStack(spacing: 22) {
            BrandMark(size: 76).clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
            VStack(spacing: 10) {
                Text("A little more off your mind.").font(.system(size: 28, weight: .semibold)).tracking(-0.5).multilineTextAlignment(.center).foregroundStyle(N.text)
                Text("Connect your projects and tools, hand off work, and see what needs your attention.")
                    .font(.system(size: 15)).foregroundStyle(.white.opacity(0.72)).multilineTextAlignment(.center).frame(maxWidth: 480)
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
            settingRow(symbol: "keyboard.fill", tint: Gem.amethyst, title: "Quick panel shortcut", detail: "Summon Neko from anywhere.") {
                HotkeySettingsView(model: model)
            }
        }
    }
    private var workspaceStep: some View {
        VStack(spacing: 14) {
        stepCard(title: "Where do you work?", subtitle: "Choose your folders. Each workspace keeps its own tools and instructions.") {
            if model.homeWorkspaceID == nil {
                Button { Task { await model.addHomeWorkspace() } } label: {
                    HStack(spacing: 14) {
                        setupIcon("house", tint: NekoStyle.accent)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Use my home folder").font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
                            Text("Recommended · one default workspace for everything in ~").font(.system(size: 13)).foregroundStyle(.white.opacity(0.65))
                        }
                        Spacer()
                        Image(systemName: "chevron.right").foregroundStyle(.white.opacity(0.5))
                    }.padding(14).contentShape(Rectangle())
                }.buttonStyle(.plain)
            }
            if model.workspaces.isEmpty {
                Button { model.selectedWorkspace = nil; workspaceSheet = true } label: {
                    HStack(spacing: 14) {
                        setupIcon("folder.badge.plus", tint: NekoStyle.accent)
                        VStack(alignment: .leading, spacing: 2) {
                            Text("Choose specific folders").font(.system(size: 15, weight: .semibold)).foregroundStyle(.white)
                            Text("A project with its own tools and instructions").font(NekoFont.body).foregroundStyle(N.text3)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").foregroundStyle(.white.opacity(0.5))
                    }.padding(14).contentShape(Rectangle())
                }.buttonStyle(.plain)
            } else {
                ForEach(model.workspaces, id: \.recordID) { item in
                    HStack(spacing: 10) {
                        Image(systemName: "folder.fill").foregroundStyle(.white.opacity(0.8))
                        Text(item["name"].string).font(.system(size: 14, weight: .medium)).foregroundStyle(.white)
                        Spacer()
                        Image(systemName: "checkmark.circle.fill").foregroundStyle(Color.oklch(0.78, 0.14, 158))
                    }.padding(.vertical, 6)
                }
                Button("Add another workspace…") { model.selectedWorkspace = nil; workspaceSheet = true }.buttonStyle(.bordered)
            }
        }
        Button { registrySheet = true } label: {
            Label("Find Sentry and other MCP servers", systemImage: "network")
                .font(.system(size: 13, weight: .medium))
        }
        .buttonStyle(.plain)
        .foregroundStyle(.white.opacity(0.8))
        .accessibilityHint("Browse hosted connections in the public MCP Registry")
        }
    }
    private func stepCard<Content: View>(title: String, subtitle: String, @ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.system(size: 24, weight: .bold)).tracking(-0.4).foregroundStyle(.white)
                Text(subtitle).font(.system(size: 14)).foregroundStyle(.white.opacity(0.7)).fixedSize(horizontal: false, vertical: true)
            }
            VStack(alignment: .leading, spacing: 4, content: content)
                .padding(12)
                .background(N.card, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        }.frame(maxWidth: 560)
    }
    private func settingRow<Trailing: View>(symbol: String, tint: Color, title: String, detail: String, @ViewBuilder trailing: () -> Trailing) -> some View {
        HStack(spacing: 14) {
            setupIcon(symbol, tint: NekoStyle.accent)
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                Text(detail).font(.system(size: 12.5)).foregroundStyle(.white.opacity(0.65))
            }
            Spacer(minLength: 12)
            trailing()
        }.padding(10)
    }
    private func setupIcon(_ symbol: String, tint: Color) -> some View {
        Image(systemName: symbol).font(.system(size: 20, weight: .regular))
            .foregroundStyle(tint).frame(width: 40, height: 40)
            .background(N.selected, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            .accessibilityHidden(true)
    }
    private func finish() { pending = true; Task { await model.completeSetup(); pending = false } }
}
