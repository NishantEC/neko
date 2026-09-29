import AppKit

/// Presents outside SwiftUI's update transaction and retains the panel until
/// completion. No nested application-modal run loop is created.
@MainActor enum FolderPicker {
    private static var panels: [UUID: NSOpenPanel] = [:]

    static func choose(multiple: Bool = false, prompt: String = "Choose folder", completion: @escaping @MainActor ([URL]) -> Void) {
        let owner = NSApp.keyWindow ?? NSApp.mainWindow
        DispatchQueue.main.async {
            let id = UUID()
            let panel = NSOpenPanel()
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.allowsMultipleSelection = multiple
            panel.canCreateDirectories = false
            panel.prompt = prompt
            panel.message = multiple ? "Choose one or more folders." : "Choose a folder."
            panels[id] = panel
            let finished: (NSApplication.ModalResponse) -> Void = { response in
                let urls = response == .OK ? panel.urls : []
                panel.orderOut(nil)
                panels.removeValue(forKey: id)
                // AppKit can invoke completion while dismissing the sheet.
                DispatchQueue.main.async { if !urls.isEmpty { completion(urls) } }
            }
            if let owner, owner.isVisible {
                var parent = owner
                while let sheet = parent.attachedSheet { parent = sheet }
                panel.beginSheetModal(for: parent, completionHandler: finished)
            } else {
                panel.begin(completionHandler: finished)
            }
        }
    }
}
