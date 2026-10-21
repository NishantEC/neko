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
    /// Content leaves. Shorter than any entrance, so dismissing never waits.
    case exit
    /// "Something is running", repeating until it stops.
    case pulse

    // Springs with no bounce: they retarget smoothly when interrupted, and
    // nothing overshoots.
    var animation: Animation {
        switch self {
        case .hover: .easeOut(duration: 0.12)
        case .press: .snappy(duration: 0.15)
        case .select: .spring(duration: 0.24, bounce: 0)
        case .reveal: .spring(duration: 0.28, bounce: 0)
        case .appear: .spring(duration: 0.36, bounce: 0)
        case .exit: .easeOut(duration: 0.15)
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

/// How something floating (a toast) arrives and leaves: a short rise and fade
/// in, a quicker fade out. Under Reduce Motion it only cross-fades, so the
/// change still reads without anything moving. The transition carries its own
/// timing, so the insert or removal itself needs no `withAnimation`.
private struct NekoEntranceModifier: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View {
        let fadeOut = AnyTransition.opacity.animation(NekoMotion.exit.animation)
        content.transition(reduceMotion
            ? fadeOut
            : .asymmetric(insertion: AnyTransition.opacity.combined(with: .offset(y: 8)).animation(NekoMotion.appear.animation),
                          removal: fadeOut))
    }
}

extension View {
    /// Animate changes of `value` with a named motion, or not at all under Reduce Motion.
    func nekoAnimation<Value: Equatable>(_ motion: NekoMotion, value: Value) -> some View {
        modifier(NekoAnimationModifier(motion: motion, value: value))
    }
    /// The insertion and removal transition for floating content. See `NekoEntranceModifier`.
    func nekoEntrance() -> some View { modifier(NekoEntranceModifier()) }
}

/// `withAnimation`, routed through a named motion and Reduce Motion.
@MainActor @discardableResult
func withNekoAnimation<Result>(_ motion: NekoMotion, _ body: () throws -> Result) rethrows -> Result {
    try withAnimation(motion.resolved(reduceMotion: NekoMotion.systemReducesMotion), body)
}
