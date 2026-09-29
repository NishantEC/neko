import SwiftUI

/// Every activity in both sizes. Reached with NEKO_START_PAGE=Activity for
/// design review; not part of the sidebar.
struct ActivityGallery: View {
    private let columns = [GridItem(.adaptive(minimum: 150), spacing: 28)]
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 36) {
                Text("Activity").font(.title2.bold()).foregroundStyle(N.text)
                LazyVGrid(columns: columns, alignment: .leading, spacing: 32) {
                    ForEach(NekoActivity.allCases) { activity in
                        VStack(spacing: 12) {
                            PixelGlyph(activity: activity, size: 30)
                            Text(activity.label).font(.body).foregroundStyle(N.text2)
                        }.frame(maxWidth: .infinity)
                    }
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 230), spacing: 24)], alignment: .leading, spacing: 28) {
                    ForEach(NekoActivity.allCases) { ActivityCapsule(activity: $0) }
                }
            }.padding(48)
        }
    }
}
