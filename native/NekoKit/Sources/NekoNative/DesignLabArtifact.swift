import SwiftUI

/// Real scrolling content gives the floating glass something to refract.
struct DesignLabArtifact: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack {
                Label("Appearance study", systemImage: "circle.lefthalf.filled")
                    .font(NekoFont.of(.callout, weight: .semibold))
                Spacer()
                Text("Draft").font(.caption).foregroundStyle(.secondary)
            }
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 14) {
                    Text("DAYLIGHT").font(NekoFont.of(.micro, weight: .semibold)).tracking(1.3)
                    Text("Less noise.\nMore clarity.").font(NekoFont.of(.headline, weight: .medium)).tracking(-0.6)
                    HStack(spacing: 8) {
                        ForEach([0.95, 0.78, 0.52, 0.16], id: \.self) { value in
                            RoundedRectangle(cornerRadius: NekoRadius.sm).fill(Color(white: value)).frame(height: 34)
                        }
                    }
                }
                .foregroundStyle(Color(white: 0.16))
                .padding(22).frame(maxWidth: .infinity, alignment: .leading)
                .background(PearlTint.lavender, in: .rect(cornerRadius: NekoRadius.xl))
                VStack(alignment: .leading, spacing: 14) {
                    Text("AFTER HOURS").font(NekoFont.of(.micro, weight: .semibold)).tracking(1.3)
                    Text("Depth without\nthe distraction.").font(NekoFont.of(.headline, weight: .medium)).tracking(-0.6)
                    HStack(spacing: 8) {
                        ForEach([0.12, 0.22, 0.40, 0.80], id: \.self) { value in
                            RoundedRectangle(cornerRadius: NekoRadius.sm).fill(Color(white: value)).frame(height: 34)
                        }
                    }
                }
                .foregroundStyle(.white)
                .padding(22).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(white: 0.14), in: .rect(cornerRadius: NekoRadius.xl))
            }
            HStack(spacing: 8) {
                Image(systemName: "paintpalette").foregroundStyle(.secondary)
                Text("Neutral surfaces").font(NekoFont.of(.meta, weight: .medium))
                Spacer()
                Text("4 shared tokens · 2 appearances").font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(22)
        .background(Color.primary.opacity(scheme == .dark ? 0.045 : 0.025), in: .rect(cornerRadius: NekoRadius.hero))
        .overlay { RoundedRectangle(cornerRadius: NekoRadius.hero).strokeBorder(.primary.opacity(0.07)) }
    }
}
