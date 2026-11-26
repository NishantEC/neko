// What one process's front window says to assistive technology, as JSON: osascript -l JavaScript scripts/qa/ax-texts.js PID
// Each element: {"role", "name", "desc", "value"} (empty strings dropped). Read without focus or the pointer. Used by the QA rig's
// --window mode (docs/knowledge/qa.md) to check what a real window shows; needs Accessibility access for the terminal.
function run(argv) {
  var pid = parseInt(argv[0], 10);
  var se = Application("System Events");
  var procs = se.processes.whose({ unixId: pid });
  if (procs.length === 0) return "[]";
  var win = procs[0].windows[0];
  var out = [];
  function walk(el, depth) {
    if (depth > 40) return;
    var kids;
    try { kids = el.uiElements(); } catch (e) { return; }
    for (var i = 0; i < kids.length; i++) {
      var k = kids[i], o = {};
      try { o.role = k.role() || ""; } catch (e) {}
      try { var n = k.name(); if (n) o.name = n; } catch (e) {}
      try { var d = k.description(); if (d) o.desc = d; } catch (e) {}
      try { var v = k.value(); if (v !== null && v !== undefined && ("" + v) !== "") o.value = "" + v; } catch (e) {}
      out.push(o);
      walk(k, depth + 1);
    }
  }
  for (var tries = 0; tries < 3 && out.length < 5; tries++) {
    if (tries > 0) delay(0.4);
    out = [];
    walk(win, 0);
  }
  return JSON.stringify(out);
}
