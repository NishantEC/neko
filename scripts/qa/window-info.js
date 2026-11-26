// The windows of one process, as JSON: osascript -l JavaScript scripts/qa/window-info.js PID
// Each: {"id": CGWindowID, "x", "y", "w", "h" (points, global), "onscreen": bool, "layer": n, "title": "…"}.
// Used by the QA rig's --window mode (docs/knowledge/qa.md) to capture that window only (screencapture -l ID).
ObjC.import("CoreGraphics");
ObjC.import("Foundation");
function run(argv) {
  var pid = parseInt(argv[0], 10);
  // kCGWindowListOptionAll: windows behind others and off screen too.
  var list = ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0));
  var out = [];
  for (var i = 0; i < list.count; i++) {
    var w = list.objectAtIndex(i);
    var owner = ObjC.unwrap(w.objectForKey("kCGWindowOwnerPID"));
    if (owner !== pid) continue;
    var b = w.objectForKey("kCGWindowBounds");
    var name = w.objectForKey("kCGWindowName");
    var on = w.objectForKey("kCGWindowIsOnscreen");
    out.push({
      id: ObjC.unwrap(w.objectForKey("kCGWindowNumber")),
      x: ObjC.unwrap(b.objectForKey("X")),
      y: ObjC.unwrap(b.objectForKey("Y")),
      w: ObjC.unwrap(b.objectForKey("Width")),
      h: ObjC.unwrap(b.objectForKey("Height")),
      onscreen: on ? ObjC.unwrap(on) === true || ObjC.unwrap(on) === 1 : false,
      layer: ObjC.unwrap(w.objectForKey("kCGWindowLayer")),
      title: name ? ObjC.unwrap(name) : ""
    });
  }
  return JSON.stringify(out);
}
