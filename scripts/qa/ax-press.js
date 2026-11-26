// Presses a control by its accessible name in one process's front window, without focus or the pointer (AXPress):
//   osascript -l JavaScript scripts/qa/ax-press.js PID "Allow once" [nth]
// Prints "pressed" or "none". Used by the QA rig's --window mode (docs/knowledge/qa.md) for controls whose keys are made at
// run time (a permission request's answers); needs Accessibility access for the terminal running the rig.
function run(argv) {
  var pid = parseInt(argv[0], 10), name = argv[1], nth = argv.length > 2 ? parseInt(argv[2], 10) : 0;
  var se = Application("System Events");
  var procs = se.processes.whose({ unixId: pid });
  if (procs.length === 0) return "none";
  var win = procs[0].windows[0];
  var found = [];
  function walk(el, depth) {
    if (depth > 40) return;
    var kids;
    try { kids = el.uiElements(); } catch (e) { return; }
    for (var i = 0; i < kids.length; i++) {
      var k = kids[i], n = "", d = "";
      try { n = k.name() || ""; } catch (e) {}
      try { d = k.description() || ""; } catch (e) {}
      // The whole name, or its start ("Worked for" names "Worked for 7s").
      var r = "";
      try { r = k.role() || ""; } catch (e) {}
      var control = r !== "AXStaticText" && r !== "AXGroup" && r !== "AXUnknown";
      if (control && (n === name || d === name || n.indexOf(name) === 0)) found.push(k);
      walk(k, depth + 1);
    }
  }
  // AccessKit builds the tree when an assistive client first asks: ask, wait a moment, and look again.
  for (var tries = 0; tries < 3 && found.length <= nth; tries++) {
    if (tries > 0) delay(0.4);
    found = [];
    walk(win, 0);
  }
  if (found.length <= nth) return "none";
  found[nth].actions.byName("AXPress").perform();
  return "pressed";
}
