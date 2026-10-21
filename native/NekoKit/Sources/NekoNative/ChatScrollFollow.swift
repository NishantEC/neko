import SwiftUI

struct ChatScrollMetrics: Equatable {
    var contentHeight: CGFloat = 0
    var originY: CGFloat = 0
    var viewportHeight: CGFloat = 0
    /// How far the newest content sits below the visible bottom edge.
    var distanceFromBottom: CGFloat { max(0, contentHeight + originY - viewportHeight) }
}

struct ChatScrollFollowState {
    private(set) var followsLatest = true
    private var previous: ChatScrollMetrics?
    private var pinnedUntil: Date = .distantPast
    /// Sending a message always jumps to it, and the insertion's intermediate
    /// offsets must not read as the user scrolling away.
    mutating func pin(for seconds: TimeInterval = 1.2, now: Date = Date()) {
        followsLatest = true
        pinnedUntil = now.addingTimeInterval(seconds)
    }
    mutating func userInteracted() {
        pinnedUntil = .distantPast
        followsLatest = false
    }
    /// Stop following without waiting for the next offset change, for a
    /// scroll the transcript itself made away from the bottom.
    mutating func release() {
        pinnedUntil = .distantPast
        followsLatest = false
    }
    mutating func update(_ metrics: ChatScrollMetrics, now: Date = Date()) {
        if now < pinnedUntil { followsLatest = true; previous = metrics; return }
        // Reply growth must not look like the reader scrolling away. Only an
        // offset change or unchanged content size updates their follow intent.
        if let previous, abs(metrics.contentHeight - previous.contentHeight) < 1 || abs(metrics.originY - previous.originY) > 1 {
            followsLatest = metrics.contentHeight + metrics.originY <= metrics.viewportHeight + 80
        }
        previous = metrics
    }
}

/// What the transcript draws from the follow state. Only this is SwiftUI
/// state, and it changes when the Jump button appears or goes, not with every
/// scrolled pixel.
struct ChatFollowVisible: Equatable {
    var showsJump = false
    /// A reply finished while you were reading further up.
    var newReply = false
}

/// Where the transcript should move after its content changed size.
enum ChatScrollAction: Equatable {
    case latest
    /// The top of a finished reply too tall to read from its last line.
    case top(String)
}

/// Follow intent for one transcript, kept in a reference type held by
/// `@State`: scroll offsets arrive every frame, and mutating a class does not
/// invalidate the view that holds it. The view mirrors `visible` into real
/// state, and only when it changes.
@MainActor final class ChatScrollTracker {
    private(set) var follow = ChatScrollFollowState()
    private(set) var metrics = ChatScrollMetrics()
    private(set) var visible = ChatFollowVisible()
    /// A reply that just finished, and how tall the content was before it did.
    private var landing: (id: String, baseline: CGFloat)?
    private var newReply = false
    private var scheduled = false
    private var pendingAction: ChatScrollAction?

    /// Takes a scroll position; reports whether the content changed size.
    @discardableResult func observe(_ next: ChatScrollMetrics, now: Date = Date()) -> Bool {
        let resized = abs(next.contentHeight - metrics.contentHeight) > 1
        metrics = next
        follow.update(next, now: now)
        refresh()
        return resized
    }

    func pin(now: Date = Date()) {
        follow.pin(now: now)
        refresh()
    }

    /// The reader moved toward older content.
    func userScrolledBack() {
        follow.userInteracted()
        refresh()
    }

    /// A pending reply was replaced by its finished text. Decide where to go
    /// once layout has measured it.
    func replyFinished(_ id: String) {
        if follow.followsLatest { landing = (id, metrics.contentHeight) }
        else { newReply = true; refresh() }
    }

    /// After the content changed size: the scroll to make, if any. A finished
    /// reply taller than the space above the dock lands on its first line;
    /// anything else keeps following the bottom.
    func actionAfterResize(dockHeight: CGFloat) -> ChatScrollAction? {
        if let landing {
            self.landing = nil
            guard follow.followsLatest else { return nil }
            let grown = metrics.contentHeight - landing.baseline
            if grown > metrics.viewportHeight - dockHeight - NekoChat.turnGap {
                follow.release()
                refresh()
                return .top(landing.id)
            }
            return .latest
        }
        return follow.followsLatest ? .latest : nil
    }

    /// Scroll requests made in one runloop collapse into one scroll, and the
    /// last one asked for wins.
    func request(_ action: ChatScrollAction,
                 schedule: (@escaping @MainActor () -> Void) -> Void = { work in DispatchQueue.main.async { MainActor.assumeIsolated(work) } },
                 perform: @escaping (ChatScrollAction) -> Void) {
        pendingAction = action
        guard !scheduled else { return }
        scheduled = true
        schedule { [weak self] in
            guard let self else { return }
            self.scheduled = false
            guard let action = self.pendingAction else { return }
            self.pendingAction = nil
            perform(action)
        }
    }

    /// A different conversation: start again at its bottom.
    func reset() {
        follow = ChatScrollFollowState()
        landing = nil
        newReply = false
        refresh()
    }

    private func refresh() {
        if follow.followsLatest { newReply = false }
        visible = ChatFollowVisible(
            showsJump: !follow.followsLatest && metrics.distanceFromBottom > metrics.viewportHeight / 2,
            newReply: newReply
        )
    }
}

enum ChatComposerClearance {
    static func bottomSpace(for composerHeight: CGFloat) -> CGFloat { composerHeight + NekoSpace.xxl }
}

/// What a transcript needs to know about its newest rows to follow them.
struct ChatTranscriptSignal: Equatable {
    var lastID: String? = nil
    /// The newest row is yours, so the transcript jumps to it.
    var lastIsUser = false
    /// The reply still being written, if any.
    var pendingID: String? = nil
}

private struct ChatDockHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

private struct ChatContentMetricsKey: PreferenceKey {
    static let defaultValue = ChatScrollMetrics()
    static func reduce(value: inout ChatScrollMetrics, nextValue: () -> ChatScrollMetrics) { value = nextValue() }
}

/// One chat, two places: Home and ticket chats. Owns the column, follow
/// scrolling, the bottom spacer and the composer dock floating over the
/// transcript. One owner for scrolling; rows never scroll anything themselves.
struct ChatTranscript<Rows: View, Dock: View>: View {
    /// Changes when a different conversation is shown.
    let identity: AnyHashable
    let signal: ChatTranscriptSignal
    var topInset: CGFloat = NekoLayout.sectionGap
    @ViewBuilder var rows: () -> Rows
    @ViewBuilder var dock: () -> Dock
    @State private var tracker = ChatScrollTracker()
    @State private var visible = ChatFollowVisible()
    @State private var dockHeight: CGFloat = 0
    private let bottomID = "chat.bottom"

    var body: some View {
        GeometryReader { outer in
            ZStack(alignment: .bottom) {
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            rows()
                            Color.clear.frame(height: ChatComposerClearance.bottomSpace(for: dockHeight)).id(bottomID)
                        }
                        .frame(maxWidth: NekoChat.chatWidth, alignment: .leading)
                        .padding(.horizontal, NekoLayout.pageInset).padding(.top, topInset)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .modifier(ChatLegacyContentMetrics(viewportHeight: outer.size.height))
                    }
                    .coordinateSpace(name: ChatLegacyContentMetrics.space)
                    .modifier(ChatScrollMetricsReader { metrics in observe(metrics, reader) })
                    .background(ChatScrollIntentObserver(
                        bottomExclusion: CGSize(width: dockWidth(outer.size.width), height: dockHeight),
                        onScrollBack: { tracker.userScrolledBack(); mirror() }
                    ))
                    .overlay(alignment: .bottom) {
                        // The transition carries its own timing, so nothing
                        // reaches the transcript's own layout.
                        ZStack {
                            if visible.showsJump || visible.newReply {
                                ChatJumpButton(newReply: visible.newReply) {
                                    tracker.pin()
                                    mirror()
                                    scroll(.latest, reader)
                                }
                                .nekoEntrance()
                            }
                        }
                        .padding(.bottom, dockHeight + NekoSpace.md)
                    }
                    .onChange(of: signal) { old, new in
                        if new.lastID != old.lastID, new.lastIsUser {
                            tracker.pin()
                            mirror()
                            scroll(.latest, reader)
                        }
                        if let finished = old.pendingID, new.pendingID != finished {
                            tracker.replyFinished(finished)
                            mirror()
                        }
                    }
                    .onChange(of: identity) { _, _ in
                        tracker.reset()
                        mirror()
                        scroll(.latest, reader)
                    }
                    .onAppear { scroll(.latest, reader) }
                }
                dock()
                    .frame(width: dockWidth(outer.size.width))
                    .padding(.bottom, NekoSpace.md).padding(.top, NekoSpace.lg)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .background(alignment: .bottom) {
                        LinearGradient(colors: [N.canvas.opacity(0), N.canvas.opacity(0.92)], startPoint: .top, endPoint: UnitPoint(x: 0.5, y: 0.45))
                            .allowsHitTesting(false)
                    }
                    .background {
                        GeometryReader { geometry in
                            Color.clear.preference(key: ChatDockHeightKey.self, value: geometry.size.height)
                        }
                    }
                    .onPreferenceChange(ChatDockHeightKey.self) { height in
                        if abs(height - dockHeight) > 1 { dockHeight = height }
                    }
            }
            // Keep scrolling transcript content below the page header.
            .clipped()
        }
    }

    private func dockWidth(_ available: CGFloat) -> CGFloat {
        min(NekoChat.chatWidth, max(280, available - NekoLayout.pageInset * 2))
    }

    private func observe(_ metrics: ChatScrollMetrics, _ reader: ScrollViewProxy) {
        let resized = tracker.observe(metrics)
        mirror()
        // Layout has now measured the change, so scroll to the new bottom
        // rather than the old one.
        if resized, let action = tracker.actionAfterResize(dockHeight: dockHeight) {
            mirror()
            scroll(action, reader)
        }
    }

    private func mirror() {
        if tracker.visible != visible { visible = tracker.visible }
    }

    private func scroll(_ action: ChatScrollAction, _ reader: ScrollViewProxy) {
        tracker.request(action) { action in
            switch action {
            case .latest: reader.scrollTo(bottomID, anchor: .bottom)
            case .top(let id): reader.scrollTo(id, anchor: .top)
            }
        }
    }
}

/// macOS 15 reads scroll geometry directly. macOS 14 measures the content's
/// frame inside the scroll view's coordinate space instead.
private struct ChatScrollMetricsReader: ViewModifier {
    let onChange: (ChatScrollMetrics) -> Void
    func body(content: Content) -> some View {
        if #available(macOS 15, *) {
            content.onScrollGeometryChange(for: ChatScrollMetrics.self) { geometry in
                ChatScrollMetrics(contentHeight: geometry.contentSize.height,
                                  originY: -geometry.contentOffset.y,
                                  viewportHeight: geometry.containerSize.height)
            } action: { _, metrics in onChange(metrics) }
        } else {
            content.onPreferenceChange(ChatContentMetricsKey.self) { metrics in onChange(metrics) }
        }
    }
}

private struct ChatLegacyContentMetrics: ViewModifier {
    static let space = "chatTranscript"
    let viewportHeight: CGFloat
    func body(content: Content) -> some View {
        if #available(macOS 15, *) {
            content
        } else {
            content.background(GeometryReader { geometry in
                Color.clear.preference(key: ChatContentMetricsKey.self, value: ChatScrollMetrics(
                    contentHeight: geometry.size.height,
                    originY: geometry.frame(in: .named(Self.space)).minY,
                    viewportHeight: viewportHeight))
            })
        }
    }
}
