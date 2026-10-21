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
    /// The message you just sent, at the top, with its reply to be written below.
    case anchor(String)
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
    /// The sent message holding the top edge while its reply is short.
    private var anchorTarget: (id: String, until: Date)?

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

    /// A message was just sent: hold it at the top while the reply is written
    /// beneath it. The hold ends when the reader scrolls or the reply fills the screen.
    func anchor(_ id: String, now: Date = Date()) {
        follow.pin(now: now)
        anchorTarget = (id, now.addingTimeInterval(1.5))
        refresh()
    }

    /// The reader moved toward older content.
    func userScrolledBack() {
        anchorTarget = nil
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
    /// `anchorHolds` says there is still room under the sent message for its
    /// reply, so the message keeps the top edge; once there is none, the
    /// chat follows the newest line like any other.
    func actionAfterResize(dockHeight: CGFloat, anchorHolds: Bool = false, now: Date = Date()) -> ChatScrollAction? {
        if let target = anchorTarget {
            if anchorHolds, now < target.until { return .anchor(target.id) }
            anchorTarget = nil
        }
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
        anchorTarget = nil
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

/// The height of the newest exchange (your last message and everything after
/// it), so the chat can leave exactly the room it needs under the top edge.
private struct ChatExchangeHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) { value = max(value, nextValue()) }
}

/// Wraps the newest exchange in a transcript that anchors what you send.
struct ChatExchange<Content: View>: View {
    @ViewBuilder var content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: 0) { content() }
            .background {
                GeometryReader { geometry in
                    Color.clear.preference(key: ChatExchangeHeightKey.self, value: geometry.size.height)
                }
            }
    }
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
    /// Sending a message puts it at the top and the reply is written below it,
    /// instead of scrolling to the bottom. The rows wrap their newest exchange in `ChatExchange`.
    var anchorsSend = false
    @ViewBuilder var rows: () -> Rows
    @ViewBuilder var dock: () -> Dock
    @State private var tracker = ChatScrollTracker()
    @State private var visible = ChatFollowVisible()
    @State private var dockHeight: CGFloat = 0
    @State private var exchangeHeight: CGFloat = 0
    /// A message was sent here, so the trailing space includes the runway under it.
    @State private var anchored = false
    private let bottomID = "chat.bottom"

    var body: some View {
        GeometryReader { outer in
            ZStack(alignment: .bottom) {
                ScrollViewReader { reader in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            rows()
                            Color.clear.frame(height: ChatComposerClearance.bottomSpace(for: dockHeight) + runway(outer.size.height)).id(bottomID)
                        }
                        .frame(maxWidth: NekoChat.chatWidth, alignment: .leading)
                        .padding(.horizontal, NekoLayout.pageInset).padding(.top, topInset)
                        .frame(maxWidth: .infinity, alignment: .center)
                        .modifier(ChatLegacyContentMetrics(viewportHeight: outer.size.height))
                    }
                    .coordinateSpace(name: ChatLegacyContentMetrics.space)
                    .onPreferenceChange(ChatExchangeHeightKey.self) { height in
                        if abs(height - exchangeHeight) > 1 { exchangeHeight = height }
                    }
                    .modifier(ChatScrollMetricsReader { metrics in observe(metrics, reader, viewport: outer.size.height) })
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
                            if anchorsSend, let id = new.lastID {
                                anchored = true
                                tracker.anchor(id)
                                mirror()
                                scroll(.anchor(id), reader)
                            } else {
                                tracker.pin()
                                mirror()
                                scroll(.latest, reader)
                            }
                        }
                        if let finished = old.pendingID, new.pendingID != finished {
                            tracker.replyFinished(finished)
                            mirror()
                        }
                    }
                    .onChange(of: identity) { _, _ in
                        anchored = false
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

    /// Space under the newest exchange that lets a sent message reach the top.
    private func runway(_ viewport: CGFloat) -> CGFloat {
        guard anchored else { return 0 }
        return ChatRunway.extra(viewport: viewport, exchange: exchangeHeight,
                                clearance: ChatComposerClearance.bottomSpace(for: dockHeight), topMargin: NekoChat.anchorMargin)
    }

    private func observe(_ metrics: ChatScrollMetrics, _ reader: ScrollViewProxy, viewport: CGFloat) {
        let resized = tracker.observe(metrics)
        mirror()
        // Layout has now measured the change, so scroll to the new bottom
        // rather than the old one.
        if resized, let action = tracker.actionAfterResize(dockHeight: dockHeight, anchorHolds: runway(viewport) > 0) {
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
            case .top(let id), .anchor(let id): reader.scrollTo(id, anchor: .top)
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
