import SwiftUI

/// The prominent "Neko is working" pill: a dark capsule with the activity's
/// pixel glyph, a shimmering label, and a coloured bloom beneath it.
struct ActivityCapsule: View {
    let activity: NekoActivity
    var label: String? = nil

    private var title: String { label ?? (activity.animates ? activity.label + "…" : activity.label) }

    var body: some View {
        HStack(spacing: 10) {
            PixelGlyph(activity: activity, size: 16)
            ShimmerText(text: title, active: activity.animates)
        }
        .padding(.leading, 12).padding(.trailing, 16).padding(.vertical, 9)
        .background {
            Capsule()
                .fill(LinearGradient(colors: [activity.color, activity.companion], startPoint: .leading, endPoint: .trailing))
                .blur(radius: 16)
                .opacity(activity.animates ? 0.55 : 0.3)
                .offset(y: 7)
                .padding(.horizontal, 10)
        }
        .background(Color.black.opacity(0.88), in: Capsule())
        .overlay { Capsule().strokeBorder(.white.opacity(0.09)) }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(title)
        .accessibilityAddTraits(.updatesFrequently)
    }
}

#Preview {
    VStack(spacing: 24) {
        ForEach(NekoActivity.allCases) { ActivityCapsule(activity: $0) }
    }.padding(40).background(N.canvas)
}
