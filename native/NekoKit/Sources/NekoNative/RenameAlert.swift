import SwiftUI

/// "Rename…" for anything Neko names on its own. An empty name hands naming
/// back to Neko, so the field never needs a separate reset control.
struct RenameAlert: ViewModifier {
    let title: String
    @Binding var isPresented: Bool
    let current: String
    let save: (String) -> Void
    @State private var draft = ""
    func body(content: Content) -> some View {
        content
            .alert(title, isPresented: $isPresented) {
                TextField("Name", text: $draft)
                Button("Save") { save(draft.trimmingCharacters(in: .whitespacesAndNewlines)) }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Leave it empty and Neko will name it for you.")
            }
            .onChange(of: isPresented) { _, shown in if shown { draft = current } }
    }
}

extension View {
    func renameAlert(_ title: String, isPresented: Binding<Bool>, current: String, save: @escaping (String) -> Void) -> some View {
        modifier(RenameAlert(title: title, isPresented: isPresented, current: current, save: save))
    }
}
