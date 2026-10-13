import SwiftUI

/// Every animation in the app, by what it confirms. All of them disappear
/// under Reduce Motion, through `.nekoAnimation` and `withNekoAnimation`.
/// Nothing scales on hover: a target that moves under the pointer is harder
/// to hit.
enum NekoMotion {
    /// Pointer enters or leaves something clickable.
    case hover
    /// A press lands.
    case press
    /// Selection moves, a tab or segment changes.
    case select
    /// Something folds open or closed.
    case reveal
    /// Content arrives on screen for the first time.
    case appear
    /// "Something is running", repeating until it stops.
    case pulse

    var animation: Animation {
        switch self {
        case .hover: .easeOut(duration: 0.12)
        case .press: .snappy(duration: 0.15)
        case .select: .snappy(duration: 0.2)
        case .reveal: .smooth(duration: 0.18)
        case .appear: .easeOut(duration: 0.35)
        case .pulse: .easeInOut(duration: 1.1).repeatForever(autoreverses: true)
        }
    }

    /// The animation to run now, or none when the user asked for less motion.
    func resolved(reduceMotion: Bool) -> Animation? { reduceMotion ? nil : animation }

    /// Reduce Motion as AppKit reports it, for code outside a view body.
    @MainActor static var systemReducesMotion: Bool { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
}

private struct NekoAnimationModifier<Value: Equatable>: ViewModifier {
    let motion: NekoMotion
    let value: Value
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View {
        content.animation(motion.resolved(reduceMotion: reduceMotion), value: value)
    }
}

extension View {
    /// Animate changes of `value` with a named motion, or not at all under Reduce Motion.
    func nekoAnimation<Value: Equatable>(_ motion: NekoMotion, value: Value) -> some View {
        modifier(NekoAnimationModifier(motion: motion, value: value))
    }
}

/// `withAnimation`, routed through a named motion and Reduce Motion.
@MainActor @discardableResult
func withNekoAnimation<Result>(_ motion: NekoMotion, _ body: () throws -> Result) rethrows -> Result {
    try withAnimation(motion.resolved(reduceMotion: NekoMotion.systemReducesMotion), body)
}
