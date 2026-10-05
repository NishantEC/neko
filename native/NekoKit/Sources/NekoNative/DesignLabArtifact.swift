import SwiftUI

/// Real scrolling content gives the floating glass something to refract.
struct DesignLabArtifact: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack {
                Label("Appearance study", systemImage: "circle.lefthalf.filled")
                    .font(.system(size: 14, weight: .semibold))
                Spacer()
                Text("Draft").font(.caption).foregroundStyle(.secondary)
            }
            HStack(spacing: 16) {
                VStack(alignment: .leading, spacing: 14) {
                    Text("DAYLIGHT").font(.system(size: 10, weight: .semibold)).tracking(1.3)
                    Text("Less noise.\nMore clarity.").font(.system(size: 25, weight: .medium)).tracking(-0.6)
                    HStack(spacing: 8) {
                        ForEach([0.95, 0.78, 0.52, 0.16], id: \.self) { value in
                            RoundedRectangle(cornerRadius: 7).fill(Color(white: value)).frame(height: 34)
                        }
                    }
                }
                .foregroundStyle(Color(white: 0.16))
                .padding(22).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(red: 0.90, green: 0.87, blue: 0.96), in: .rect(cornerRadius: 18))
                VStack(alignment: .leading, spacing: 14) {
                    Text("AFTER HOURS").font(.system(size: 10, weight: .semibold)).tracking(1.3)
                    Text("Depth without\nthe distraction.").font(.system(size: 25, weight: .medium)).tracking(-0.6)
                    HStack(spacing: 8) {
                        ForEach([0.12, 0.22, 0.40, 0.80], id: \.self) { value in
                            RoundedRectangle(cornerRadius: 7).fill(Color(white: value)).frame(height: 34)
                        }
                    }
                }
                .foregroundStyle(.white)
                .padding(22).frame(maxWidth: .infinity, alignment: .leading)
                .background(Color(white: 0.14), in: .rect(cornerRadius: 18))
            }
            HStack(spacing: 8) {
                Image(systemName: "paintpalette").foregroundStyle(.secondary)
                Text("Neutral surfaces").font(.system(size: 13, weight: .medium))
                Spacer()
                Text("4 shared tokens · 2 appearances").font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(22)
        .background(Color.primary.opacity(scheme == .dark ? 0.045 : 0.025), in: .rect(cornerRadius: 24))
        .overlay { RoundedRectangle(cornerRadius: 24).strokeBorder(.primary.opacity(0.07)) }
    }
}
