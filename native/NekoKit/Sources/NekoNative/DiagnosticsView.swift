import SwiftUI
import NekoKit

struct DiagnosticCheck: Identifiable, Equatable {
    let name: String
    let ok: Bool
    let detail: String
    let millis: Int
    var id: String { name }
    static func parse(_ value: JSONValue) -> [DiagnosticCheck] {
        value["checks"].array.map { DiagnosticCheck(name: $0["name"].string, ok: $0["ok"].bool, detail: $0["detail"].string, millis: $0["millis"].int) }
    }
}

/// Times every runtime Neko depends on and shows which installs it uses.
/// Reads metadata only; nothing here sends a prompt.
struct DiagnosticsView: View {
    @ObservedObject var model: AppModel
    @State private var checks: [DiagnosticCheck] = []
    @State private var roundTrip: Int?
    @State private var running = false
    var body: some View {
        Form {
            Section {
                HStack {
                    Text("Checks installed runtimes and response times. No model request is sent.")
                        .font(.callout).foregroundStyle(.secondary)
                    Spacer()
                    Button(running ? "Running…" : "Run diagnostics") { run() }.disabled(running)
                }
                if let roundTrip {
                    LabeledContent("App ↔ daemon") { Text("\(roundTrip) ms").monospacedDigit().foregroundStyle(.secondary) }
                }
                if checks.isEmpty && !running {
                    Label("Run a check to see connection and runtime details.", systemImage: "stethoscope")
                        .font(NekoFont.body).foregroundStyle(.secondary).padding(.vertical, 12)
                }
                ForEach(checks) { check in
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: check.ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(check.ok ? Color.green : NekoStyle.amber)
                        VStack(alignment: .leading, spacing: 5) {
                            Text(check.name).font(NekoFont.heading)
                            Text(check.detail).font(NekoFont.meta).foregroundStyle(.secondary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer()
                        if check.millis > 0 { Text("\(check.millis) ms").font(.caption).monospacedDigit().foregroundStyle(.secondary) }
                    }.padding(.vertical, 6)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
    }
    private func run() {
        running = true
        Task {
            let started = Date()
            _ = try? await model.request(.string("Ping"))
            roundTrip = Int(Date().timeIntervalSince(started) * 1000)
            do { checks = DiagnosticCheck.parse(try await model.request(.string("Diagnostics"))["Diagnostics"]) }
            catch { model.error = error.localizedDescription }
            running = false
        }
    }
}
