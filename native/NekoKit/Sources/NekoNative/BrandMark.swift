import SwiftUI

struct BrandMark: View {
    var size: CGFloat = 28
    var body: some View {
        Image(nsImage: NSApp.applicationIconImage)
            .resizable().scaledToFit().frame(width: size, height: size)
            .accessibilityHidden(true)
    }
}
