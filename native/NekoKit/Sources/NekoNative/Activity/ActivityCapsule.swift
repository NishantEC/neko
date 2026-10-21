import SwiftUI

/// The inline "Neko is working" indicator: the activity's pixel glyph and a
/// shimmering label, sitting directly on the transcript.
struct ActivityCapsule: View {
    let activity: NekoActivity
    var label: String? = nil
    /// In-line in a page: just the glyph and the shimmering words, no pill.
    var plain = false
    /// The label's type; the default suits a headline status, a log line passes `meta`.
    var font: Font = NekoFont.of(.callout, weight: .medium)

    private var title: String { label ?? (activity.animates ? activity.label + "…" : activity.label) }

    var body: some View {
        if plain {
            HStack(spacing: 8) {
                PixelGlyph(activity: activity, size: 14)
                ShimmerText(text: title, active: activity.animates, font: font)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(title)
            .accessibilityAddTraits(.updatesFrequently)
        } else {
            pill
        }
    }

    private var pill: some View {
        HStack(spacing: 10) {
            PixelGlyph(activity: activity, size: 16)
            ShimmerText(text: title, active: activity.animates)
        }
        .padding(.vertical, 4)
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
