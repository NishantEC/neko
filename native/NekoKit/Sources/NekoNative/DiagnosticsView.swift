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
                    Text("Checks the agent runtimes, the Codex install Neko uses, and how long each takes to answer. Nothing is sent to a model.")
                        .font(.callout).foregroundStyle(.secondary)
                    Spacer()
                    Button(running ? "Running…" : "Run diagnostics") { run() }.disabled(running)
                }
                if let roundTrip {
                    LabeledContent("App ↔ daemon") { Text("\(roundTrip) ms").monospacedDigit().foregroundStyle(.secondary) }
                }
                ForEach(checks) { check in
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: check.ok ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
                            .foregroundStyle(check.ok ? Color.green : NekoStyle.amber)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(check.name).font(.headline)
                            Text(check.detail).font(.caption).foregroundStyle(.secondary).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer()
                        if check.millis > 0 { Text("\(check.millis) ms").font(.caption).monospacedDigit().foregroundStyle(.secondary) }
                    }
                }
            }
        }
        .formStyle(.grouped)
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

