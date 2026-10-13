import XCTest
import NekoKit
@testable import NekoNative

final class ConnectionFormTests: XCTestCase {
    func testAddNeedsANameADestinationAndTrustForLocalExecutables() {
        var form = ConnectionForm()
        XCTAssertFalse(form.canSubmit)
        form.label = "  "
        form.target = "https://example.com/mcp"
        XCTAssertFalse(form.canSubmit, "A blank name is not a name")
        form.label = "Linear"
        XCTAssertTrue(form.canSubmit)
        form.local = true
        XCTAssertFalse(form.canSubmit, "A local executable needs explicit trust")
        form.trust = true
        XCTAssertTrue(form.canSubmit)
    }

    func testHostedConfigurationNeedsACompleteURL() {
        var form = ConnectionForm(label: "Linear", target: "example.com/mcp")
        XCTAssertEqual(form.configuration(), .failure(.init(message: "Enter a complete HTTP or HTTPS server URL.")))
        form.target = " https://example.com/mcp "
        XCTAssertEqual(form.configuration(), .success(.object(["transport": .string("http"), "url": .string("https://example.com/mcp")])))
    }

    func testLocalConfigurationChecksPathArgumentsAndFolder() {
        var form = ConnectionForm(label: "Local", local: true, target: "relative/tool", trust: true)
        XCTAssertEqual(form.configuration(), .failure(.init(message: "Use an absolute executable path.")))
        form.target = "/usr/bin/env"
        form.arguments = "not json"
        XCTAssertEqual(form.configuration(), .failure(.init(message: "Arguments must be a JSON array of strings.")))
        form.arguments = #"["--stdio"]"#
        form.workingDirectory = "/no/such/folder/for/neko"
        XCTAssertEqual(form.configuration(), .failure(.init(message: "Choose an existing absolute working directory.")))
        form.workingDirectory = ""
        XCTAssertEqual(form.configuration(), .success(.object([
            "transport": .string("stdio"), "command": .string("/usr/bin/env"),
            "args": .array([.string("--stdio")]), "cwd": .null
        ])))
    }

    func testScopeAndClientID() {
        var form = ConnectionForm()
        XCTAssertEqual(form.scope(in: "w1"), "w1")
        XCTAssertEqual(form.scope(in: ""), "")
        form.global = true
        XCTAssertEqual(form.scope(in: "w1"), "")
        XCTAssertEqual(ConnectionForm.clientIDValue("  "), .null)
        XCTAssertEqual(ConnectionForm.clientIDValue(" abc "), .string("abc"))
    }
}
