import SwiftUI

struct DesignLabView: View {
    @State private var preview = DesignLabModel()
    @State private var composerHeight: CGFloat = 200

    var body: some View {
        ScrollViewReader { scroll in
            ScrollView {
                VStack(spacing: 0) {
                    DesignLabConversation(messages: preview.messages)
                        .frame(maxWidth: 820)
                        .padding(.horizontal, 44).padding(.top, 32)
                        .frame(maxWidth: .infinity)
                        .id("preview-top")
                    // Include the overlay clearance in the scroll target so sent text stays visible.
                    Color.clear.frame(height: composerHeight + 52).id("preview-end")
                }
            }
            .overlay(alignment: .bottom) {
                DesignLabComposer(preview: preview)
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { composerHeight = $0 }
                    .frame(maxWidth: 880)
                    .padding(.horizontal, 32).padding(.bottom, 24)
            }
            .onChange(of: preview.messages.count) { _, _ in
                if preview.messages.isEmpty {
                    scroll.scrollTo("preview-top", anchor: .top)
                } else {
                    scroll.scrollTo("preview-end", anchor: .bottom)
                }
            }
        }
        .background(Color(nsColor: .textBackgroundColor))
        .navigationTitle("Design lab")
        .navigationSubtitle("Liquid Glass · local preview")
        .toolbar {
            ToolbarItem(placement: .principal) {
                Picker("Composer layout", selection: $preview.layout) {
                    ForEach(DesignLabModel.Layout.allCases) { Text($0.rawValue).tag($0) }
                }
                .pickerStyle(.segmented).frame(width: 200)
            }
            ToolbarItem(placement: .primaryAction) {
                Button("Reset preview", systemImage: "arrow.counterclockwise", action: preview.reset)
                    .labelStyle(.iconOnly).help("Reset this local preview")
            }
        }
        .sheet(item: $preview.selectedAgent) { agent in
            DesignLabAgentSheet(agent: agent)
        }
        .sheet(isPresented: $preview.showingLibrary) {
            DesignLabPluginLibrary(preview: preview)
        }
    }
}
