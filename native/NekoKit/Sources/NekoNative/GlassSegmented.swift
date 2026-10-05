import SwiftUI

/// Kept as the shared call-site API; AppKit owns selection, keyboard
/// navigation, focus and appearance instead of a custom animated glass thumb.
struct GlassSegmented<Value: Hashable>: View {
    struct Option: Identifiable {
        let value: Value
        let title: String
        var symbol: String? = nil
        var help: String? = nil
        var id: Value { value }
    }
    @Binding var selection: Value
    let options: [Option]
    var iconOnly = false
    var size: ControlSize = .regular

    var body: some View {
        Picker("View", selection: $selection) {
            ForEach(options) { option in
                optionLabel(option)
                    .tag(option.value)
                    .help(option.help ?? option.title)
            }
        }
        .pickerStyle(.segmented)
        .labelsHidden()
        .controlSize(size)
        .frame(height: NekoControlMetrics.height(for: size))
        .fixedSize(horizontal: true, vertical: false)
    }

    @ViewBuilder private func optionLabel(_ option: Option) -> some View {
        if iconOnly, let symbol = option.symbol {
            Label(option.title, systemImage: symbol).labelStyle(.iconOnly)
        } else {
            Text(option.title)
        }
    }
}
