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
        VStack(alignment: .leading, spacing: 20) {
            Text(original == .null ? "Create workspace" : "Workspace settings").font(.title2.bold())
            TextField("Workspace name", text: $name).textFieldStyle(.roundedBorder)
            Text("Source folders").font(.headline)
            List {
                ForEach(folders, id: \.self) { path in
                    HStack { Image(systemName: "folder"); Text(path).lineLimit(2).textSelection(.enabled); Spacer(); Button { folders.removeAll { $0 == path } } label: { Image(systemName: "minus.circle") }.accessibilityLabel("Remove \(path)") }
                }
            }.frame(height: 150).overlay { if folders.isEmpty { Text("Add the folders where your work lives.").foregroundStyle(.secondary) } }
            Button("Add folders…", systemImage: "folder.badge.plus") { chooseFolders() }
            TextField("Instructions for this workspace (optional)", text: $instructions, axis: .vertical).lineLimit(3...6).textFieldStyle(.roundedBorder)
            Text("Folders do not need to be Git repositories. Existing tools and skills stay in their original locations.").font(.callout).foregroundStyle(.secondary)
            HStack { Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction); Spacer(); Button(saving ? "Saving…" : "Save workspace") { Task { await save() } }.keyboardShortcut(.defaultAction).disabled(saving || name.trimmingCharacters(in: .whitespaces).isEmpty || folders.isEmpty) }
        }.padding(28).frame(width: 540)
            .onAppear {
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
