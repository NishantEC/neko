import SwiftUI
import NekoKit

/// The rulebook every agent works to: Neko itself and every task agent it
/// hands work to. Plain words, edited in one place.
struct RulebookView: View {
    @ObservedObject var model: AppModel
    @State private var draft = ""
    @State private var loaded = false
    @State private var saving = false
    private var saved: String {
        let custom = model.snapshot["rulebook"].string
        return custom.isEmpty ? model.snapshot["rulebook_default"].string : custom
    }
    private var usingDefault: Bool { model.snapshot["rulebook"].string.isEmpty }
    var body: some View {
        NekoPage(title: "Rulebook", message: "Neko and every agent it hands work to follow these rules: how to engineer a change, how to review one, how to treat this Mac and your repositories. Write them in plain words.") {
            EmptyView()
        } content: {
            VStack(alignment: .leading, spacing: NekoSpace.md) {
                HStack {
                    Label(usingDefault ? "Using Neko's default rules" : "Using your own rules", systemImage: usingDefault ? "book.closed" : "pencil")
                        .font(NekoFont.meta).foregroundStyle(N.text3)
                    Spacer()
                    if !usingDefault {
                        Button("Restore default") { save("") }.nekoButton(.quiet).disabled(saving)
                    }
                    Button(saving ? "Saving…" : "Save") { save(draft) }
                        .nekoButton(.primary).disabled(saving || draft == saved)
                        .keyboardShortcut("s", modifiers: .command)
                }
                TextEditor(text: $draft)
                    .font(NekoFont.mono)
                    .scrollContentBackground(.hidden)
                    .padding(NekoSpace.md)
                    .frame(minHeight: 420)
                    .nekoFill(N.panel, radius: NekoRadius.lg)
                    .accessibilityLabel("Rulebook")
                Text("Changes apply to the next agent that starts; agents already running keep the rules they started with.")
                    .font(NekoFont.meta).foregroundStyle(N.text3)
            }
        }
        .onAppear { if !loaded { draft = saved; loaded = true } }
        .onChange(of: saved) { old, new in if draft == old { draft = new } }
    }
    private func save(_ text: String) {
        saving = true
        Task {
            if await model.workbench(.command("SetRulebook", ["text": .string(text)])) { draft = saved }
            saving = false
        }
    }
}
