import SwiftUI
import NekoKit

/// Development page (NEKO_START_PAGE="Reply views"): every reply view with
/// sample data, so the views can be checked without sending real messages.
struct ReplyGallery: View {
    static let sample = """
    Changing the address clears the slot people already picked. They have to choose again, and 41% leave at that point.

    ```neko-chart
    {"title":"Funnel, last 7 days","source":"CleverTap","bars":[{"label":"Viewed test","value":12480},{"label":"Picked address","value":8915},{"label":"Picked slot","value":6102},{"label":"Re-picked slot","value":3601,"highlight":true},{"label":"Booked","value":3377}]}
    ```

    ```neko-sources
    {"sources":["Sentry","CleverTap","SlotPicker.tsx:88","3b1f2c0"]}
    ```

    | City | Before | After | Change |
    |---|--:|--:|--:|
    | Pune | 30.1% | 22.0% | −8.1 |
    | Bengaluru | 31.2% | 24.8% | −6.4 |
    | Delhi NCR | 27.4% | 27.9% | +0.5 |

    ```console
    $ pnpm test funnel/slots
    ✓ slots/available.test.ts (6)
    ✓ slots/timezone.test.ts (5)
    ✗ slots/select.test.ts › keeps the chosen slot after address change
      Expected slotId "s_0930", received undefined
    ```

    ```diff
    diff --git a/src/funnel/SlotPicker.tsx b/src/funnel/SlotPicker.tsx
    @@ -86,4 +86,5 @@ function onAddressChange(next: Address) {
    -  setSlot(undefined)
    +  const stillOffered = slotsFor(next).some(s => s.id === slot?.id)
    +  if (!stillOffered) setSlot(undefined)
       setAddress(next)
    ```

    ```neko-choices
    {"question":"Which branch should the fix go on?","options":[{"label":"feat/diagnostic-test-funnel","detail":"Your branch, 45 files ahead"},{"label":"main","detail":"Hotfix, ships with the next release"},{"label":"New branch","detail":"neko/slot-keep"}]}
    ```

    ```neko-form
    {"title":"Watch the diagnostic funnel","submit":"Start Watching","fields":[{"label":"Alert when booked rate is below","kind":"text","value":"24%"},{"label":"Check","kind":"choice","options":["Every morning","Every hour"]},{"label":"Fix low-risk issues on its own","kind":"toggle","value":true}]}
    ```

    ```neko-plan
    {"title":"Plan","estimate":"about 25 min","steps":["Reproduce the drop with a failing test","Keep the slot when it’s still offered","Add a CleverTap event for re-picks","Run the funnel suite and review"]}
    ```

    ```neko-files
    {"files":[{"path":"/Applications/Neko.app","note":"installed app"},{"path":"~/Documents","note":"your documents"}]}
    ```
    """
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Reply views").font(.system(size: 15, weight: .semibold))
                ReplySteps(calls: [
                    .object(["id": .string("1"), "tool_name": .string("search_issues"), "status": .string("succeeded"), "connection_id": .string("")]),
                    .object(["id": .string("2"), "tool_name": .string("list_events"), "status": .string("succeeded"), "connection_id": .string("")]),
                    .object(["id": .string("3"), "tool_name": .string("run_query"), "status": .string("failed"), "connection_id": .string("")]),
                ], receipts: [], connectionName: { _ in "" }, workedFor: "Worked for 3m 02s")
                ReadableText(text: ReplyGallery.sample).font(.system(size: 13)).lineSpacing(3)
                ReplyErrorCallout(message: "Codex went quiet for 5 minutes while reading 1,200 files.", retry: {})
                ReplyMemoryNote(text: "test funnel fixes against CleverTap, not only unit tests.")
                ReplyTicketRow(title: "Keep slot after address change", status: "AwaitingApproval", workspace: "hme/athena") {}
            }
            .padding(.horizontal, 32).padding(.vertical, 24).frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        }
    }
}

