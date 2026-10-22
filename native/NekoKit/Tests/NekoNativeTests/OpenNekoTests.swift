import XCTest
@testable import NekoNative

final class OpenNekoTests: XCTestCase {
    private func dataDir(token: String?) throws -> [String: String] {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        if let token { try token.write(to: dir.appendingPathComponent("web-token"), atomically: true, encoding: .utf8) }
        return ["NEKO_DATA_DIR": dir.path, "NEKO_WEB_PORT": "4999"]
    }

    func testBrowserURLSignsInAndCarriesTheRoute() throws {
        let token = String(repeating: "ab", count: 32)
        let env = try dataDir(token: token)
        XCTAssertEqual(OpenNeko.browserURL(environment: env)?.absoluteString, "http://127.0.0.1:4999/?token=\(token)")
        XCTAssertEqual(OpenNeko.browserURL(route: "settings", environment: env)?.absoluteString, "http://127.0.0.1:4999/?token=\(token)#/settings")
    }

    func testNoURLWithoutAValidToken() throws {
        XCTAssertNil(OpenNeko.browserURL(environment: try dataDir(token: nil)))
        XCTAssertNil(OpenNeko.browserURL(environment: try dataDir(token: "short")))
        var off = try dataDir(token: String(repeating: "ab", count: 32))
        off["NEKO_WEB_PORT"] = "0"
        XCTAssertNil(OpenNeko.browserURL(environment: off), "a disabled bridge has no address")
    }

    func testBrowserIsTheDefault() {
        XCTAssertEqual(OpenNeko(rawValue: ""), nil)
        XCTAssertEqual(OpenNeko(rawValue: "") ?? .browser, .browser)
    }
}
