import SwiftUI
import NekoKit

struct WorkspaceEditor: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var instructions = ""
    @State private var folders: [String] = []
    @State private var original: JSONValue = .null
    @State private var saving = false
    var body: some View {
        NekoSheet(title: original == .null ? "Create workspace" : "Workspace settings",
                  message: "Give this work a name, a home, and its own instructions.", error: model.error) {
            Form {
                Section("Workspace") {
                    TextField("Name", text: $name).textFieldStyle(.roundedBorder)
                }
                Section {
                    if folders.isEmpty {
                        NekoEmptyState("Choose at least one folder where this work lives.", reason: .nothingYet)
                    }
                    ForEach(folders, id: \.self) { path in
                        HStack(alignment: .top, spacing: 10) {
                            Image(systemName: "folder").foregroundStyle(N.text3).accessibilityHidden(true)
                            Text(verbatim: (path as NSString).abbreviatingWithTildeInPath)
                                .font(NekoFont.body).textSelection(.enabled)
                                .fixedSize(horizontal: false, vertical: true)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            Button("Remove folder", systemImage: "minus.circle") { folders.removeAll { $0 == path } }
                                .labelStyle(.iconOnly).nekoButton(.quiet)
                                .accessibilityLabel("Remove \(path)").help("Remove \(path)")
                        }.padding(.vertical, NekoSpace.xs)
                    }
                    Button("Add folders…", systemImage: "folder.badge.plus") { chooseFolders() }
                } header: { Text("Source folders") } footer: {
                    Text("Folders do not need to be Git repositories. Existing tools and skills stay in their original locations.")
                }
                Section {
                    TextField("How should Neko work here?", text: $instructions, axis: .vertical)
                        .lineLimit(4...8).textFieldStyle(.roundedBorder)
                        .multilineTextAlignment(.leading)
                        .accessibilityLabel("Workspace instructions, optional")
                } header: { Text("Instructions · optional") }
            }
            .formStyle(.grouped).scrollContentBackground(.hidden).frame(height: 420)
        } actions: {
            NekoPrimaryAction(title: saving ? "Saving…" : "Save workspace",
                              disabled: saving || name.trimmingCharacters(in: .whitespaces).isEmpty || folders.isEmpty) { Task { await save() } }
        }.onAppear {
            if let selected = model.selectedWorkspace, let workspace = model.workspaces.first(where: { $0.recordID == selected }) {
                original = workspace; name = workspace["name"].string; instructions = workspace["instructions"].string
                folders = model.snapshot["workspace_folders"][selected].array.map(\.string)
                if folders.isEmpty { folders = [workspace["repository"].string] }
            }
        }
    }
    private func chooseFolders() {
        FolderPicker.choose(multiple: true, prompt: "Add folders") { selected in
            for url in selected where !folders.contains(url.path) { folders.append(url.path) }
            if name.isEmpty { name = selected.first?.lastPathComponent ?? "" }
        }
    }
    private func save() async {
        saving = true
        let id = original == .null ? "" : original.recordID
        let workspace: JSONValue = .object(["id": .string(id), "name": .string(name), "repository": .string(folders[0]), "instructions": .string(instructions), "away_enabled": .bool(original["away_enabled"].bool)])
        let saved = await model.workbench(.command("SaveWorkspaceWithFolders", ["workspace": workspace, "folders": .array(folders.map(JSONValue.string))]))
        saving = false
        if saved {
            model.selectedWorkspace = id.isEmpty ? model.workspaces.first { $0["repository"].string == folders[0] }?.recordID : id
            dismiss()
        }
    }
}
